// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/cfwl_scrollbar.{h,cpp}`. Geometry and source input ordering are
//! retained; cells keep every host/event/timer callback outside mutable borrows.
use super::{
    cfwl_app::CfwlApp,
    cfwl_event::{FwlEvent, ScrollCode},
    cfwl_message::{FwlMessage, MessageData, MouseCommand},
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
    cfwl_widget::{CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef, WidgetState},
};
use crate::{
    fxcrt::{
        cfx_timer::{CfxTimer, TimerCallback},
        fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, CfxVector},
    },
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub const FWL_STYLEEXT_SCB_HORZ: u32 = 0;
pub const FWL_STYLEEXT_SCB_VERT: u32 = 1;
const SCROLLBAR_ELAPSED_MSECS: i32 = 500;
const MIN_THUMB_SIZE: f32 = 5.;

pub struct CfwlScrollBar {
    base: CfwlWidget,
    weak_self: Weak<Self>,
    range_min: Cell<f32>,
    range_max: Cell<f32>,
    page_size: Cell<f32>,
    step_size: Cell<f32>,
    pos: Cell<f32>,
    track_pos: Cell<f32>,
    part_states: [Cell<PartState>; 5],
    last_track_pos: Cell<f32>,
    track_point: Cell<CfxPointF>,
    mouse_wheel: Cell<i32>,
    button_len: Cell<f32>,
    mouse_down: Cell<bool>,
    min_size: Cell<bool>,
    client_rect: Cell<CfxRectF>,
    thumb_rect: Cell<CfxRectF>,
    min_btn_rect: Cell<CfxRectF>,
    max_btn_rect: Cell<CfxRectF>,
    min_track_rect: Cell<CfxRectF>,
    max_track_rect: Cell<CfxRectF>,
    timer: RefCell<Option<Rc<CfxTimer>>>,
    timer_generation: Cell<u64>,
}
impl CfwlScrollBar {
    pub fn new(app: &Rc<CfwlApp>, properties: Properties, outer: Option<WidgetRef>) -> Rc<Self> {
        let bar = Rc::new_cyclic(|weak_self| Self {
            base: CfwlWidget::new(app, properties, outer),
            weak_self: weak_self.clone(),
            range_min: Cell::new(0.),
            range_max: Cell::new(-1.),
            page_size: Cell::new(0.),
            step_size: Cell::new(0.),
            pos: Cell::new(0.),
            track_pos: Cell::new(0.),
            part_states: std::array::from_fn(|_| Cell::new(PartState::NORMAL)),
            last_track_pos: Cell::new(0.),
            track_point: Cell::new(CfxPointF::default()),
            mouse_wheel: Cell::new(0),
            button_len: Cell::new(0.),
            mouse_down: Cell::new(false),
            min_size: Cell::new(false),
            client_rect: Cell::new(CfxRectF::default()),
            thumb_rect: Cell::new(CfxRectF::default()),
            min_btn_rect: Cell::new(CfxRectF::default()),
            max_btn_rect: Cell::new(CfxRectF::default()),
            min_track_rect: Cell::new(CfxRectF::default()),
            max_track_rect: Cell::new(CfxRectF::default()),
            timer: RefCell::new(None),
            timer_generation: Cell::new(0),
        });
        app.register_widget(bar.clone() as WidgetRef);
        bar
    }
    pub fn get_range(&self) -> (f32, f32) {
        (self.range_min.get(), self.range_max.get())
    }
    pub fn set_range(&self, min: f32, max: f32) {
        if min.is_finite() && max.is_finite() {
            self.range_min.set(min);
            self.range_max.set(max);
        }
    }
    pub fn get_page_size(&self) -> f32 {
        self.page_size.get()
    }
    pub fn set_page_size(&self, size: f32) {
        if size.is_finite() {
            self.page_size.set(size);
        }
    }
    pub fn get_step_size(&self) -> f32 {
        self.step_size.get()
    }
    pub fn set_step_size(&self, size: f32) {
        if size.is_finite() {
            self.step_size.set(size);
        }
    }
    pub fn get_pos(&self) -> f32 {
        self.pos.get()
    }
    pub fn set_pos(&self, pos: f32) {
        if pos.is_finite() {
            self.pos.set(pos);
        }
    }
    pub fn get_track_pos(&self) -> f32 {
        self.track_pos.get()
    }
    pub fn set_track_pos(&self, pos: f32) {
        if !pos.is_finite() {
            return;
        }
        self.track_pos.set(pos);
        self.thumb_rect
            .set(self.calc_thumb_button_rect(self.thumb_rect.get()));
        self.min_track_rect
            .set(self.calc_min_track_rect(self.min_track_rect.get()));
        self.max_track_rect
            .set(self.calc_max_track_rect(self.max_track_rect.get()));
    }
    pub fn is_vertical(&self) -> bool {
        self.base.get_style_exts() & FWL_STYLEEXT_SCB_VERT != 0
    }
    pub fn do_scroll(&self, code: ScrollCode, pos: f32) -> bool {
        if code == ScrollCode::None || !pos.is_finite() {
            return false;
        }
        self.on_scroll(code, pos)
    }
    fn draw_part(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        part: Part,
        rect: CfxRectF,
        state: PartState,
        arrow: bool,
    ) {
        if arrow && (rect.width <= 0. || rect.height <= 0.) {
            return;
        }
        let Some(app) = self.base.app() else {
            return;
        };
        let mut params = CfwlThemeBackground::new(part, self);
        params.matrix = matrix;
        params.part_rect = rect;
        params.states = if self.base.get_states().contains(WidgetState::DISABLED) {
            PartState::DISABLED
        } else {
            state
        };
        app.theme_provider().draw_background(&params, graphics);
    }
    fn layout(&self) {
        self.client_rect.set(self.get_client_rect());
        self.calc_button_len();
        self.min_btn_rect.set(self.calc_min_button_rect());
        self.max_btn_rect.set(self.calc_max_button_rect());
        self.thumb_rect
            .set(self.calc_thumb_button_rect(self.thumb_rect.get()));
        self.min_track_rect
            .set(self.calc_min_track_rect(self.min_track_rect.get()));
        self.max_track_rect
            .set(self.calc_max_track_rect(self.max_track_rect.get()));
    }
    fn calc_button_len(&self) {
        let client = self.client_rect.get();
        let mut button = if self.is_vertical() {
            client.width
        } else {
            client.height
        };
        let length = if self.is_vertical() {
            client.height
        } else {
            client.width
        };
        let min_size = length < button * 2.;
        if min_size {
            button = length / 2.;
        }
        self.button_len.set(button.max(0.));
        self.min_size.set(min_size);
    }
    fn calc_min_button_rect(&self) -> CfxRectF {
        let c = self.client_rect.get();
        if self.is_vertical() {
            CfxRectF::new(c.left, c.top, c.width, self.button_len.get())
        } else {
            CfxRectF::new(c.left, c.top, self.button_len.get(), c.height)
        }
    }
    fn calc_max_button_rect(&self) -> CfxRectF {
        let c = self.client_rect.get();
        if self.is_vertical() {
            CfxRectF::new(
                c.left,
                c.bottom() - self.button_len.get(),
                c.width,
                self.button_len.get(),
            )
        } else {
            CfxRectF::new(
                c.right() - self.button_len.get(),
                c.top,
                self.button_len.get(),
                c.height,
            )
        }
    }
    fn calc_thumb_button_rect(&self, old: CfxRectF) -> CfxRectF {
        if !self.base.is_enabled() {
            return CfxRectF::default();
        }
        if self.min_size.get() {
            return CfxRectF::new(old.left, old.top, 0., 0.);
        }
        let c = self.client_rect.get();
        let range = self.range_max.get() - self.range_min.get();
        if range < 0. {
            let max = self.max_btn_rect.get();
            return if self.is_vertical() {
                CfxRectF::new(c.left, max.bottom(), c.width, 0.)
            } else {
                CfxRectF::new(max.right(), c.top, 0., c.height)
            };
        }
        let mut length = if self.is_vertical() {
            c.height
        } else {
            c.width
        };
        let size = self.button_len.get();
        length -= size * 2.;
        if length < size {
            length = 0.;
        }
        // A zero range has no movable thumb, and avoids the source's 0/0.
        if range == 0. || !range.is_finite() {
            return CfxRectF::default();
        }
        let thumb_size = (length * length / (range + length)).max(MIN_THUMB_SIZE);
        let diff = (length - thumb_size).max(0.);
        let track = self
            .track_pos
            .get()
            .clamp(self.range_min.get(), self.range_max.get());
        let offset = size + diff * (track - self.range_min.get()) / range;
        if self.is_vertical() {
            CfxRectF::new(c.left, c.top + offset, c.width, thumb_size)
        } else {
            CfxRectF::new(c.left + offset, c.top, thumb_size, c.height)
        }
    }
    fn calc_min_track_rect(&self, old: CfxRectF) -> CfxRectF {
        if self.min_size.get() {
            return CfxRectF::new(old.left, old.top, 0., 0.);
        }
        let c = self.client_rect.get();
        let thumb = self.thumb_rect.get();
        // Track dimensions are extents, not absolute midpoint coordinates.
        if self.is_vertical() {
            CfxRectF::new(
                c.left,
                c.top,
                c.width,
                ((thumb.top + thumb.bottom()) / 2. - c.top).max(0.),
            )
        } else {
            CfxRectF::new(
                c.left,
                c.top,
                ((thumb.left + thumb.right()) / 2. - c.left).max(0.),
                c.height,
            )
        }
    }
    fn calc_max_track_rect(&self, old: CfxRectF) -> CfxRectF {
        if self.min_size.get() {
            return CfxRectF::new(old.left, old.top, 0., 0.);
        }
        let c = self.client_rect.get();
        let thumb = self.thumb_rect.get();
        if self.is_vertical() {
            let y = (thumb.top + thumb.bottom()) / 2.;
            CfxRectF::new(c.left, y, c.width, (c.bottom() - y).max(0.))
        } else {
            let x = (thumb.left + thumb.right()) / 2.;
            CfxRectF::new(x, c.top, (c.right() - x).max(0.), c.height)
        }
    }
    fn get_track_point_pos(&self, point: CfxPointF) -> f32 {
        let min = self.range_min.get();
        let max = self.range_max.get();
        if max < min {
            return min;
        }
        let diff = point - self.track_point.get();
        let (delta, length) = if self.is_vertical() {
            (
                diff.y,
                self.max_btn_rect.get().top
                    - self.min_btn_rect.get().bottom()
                    - self.thumb_rect.get().height,
            )
        } else {
            (
                diff.x,
                self.max_btn_rect.get().left
                    - self.min_btn_rect.get().right()
                    - self.thumb_rect.get().width,
            )
        };
        if length <= 0. || !length.is_finite() {
            return self.last_track_pos.get().clamp(min, max);
        }
        ((max - min) * delta / length + self.last_track_pos.get()).clamp(min, max)
    }
    fn send_event(&self) -> bool {
        if self.part_states[0].get() == PartState::PRESSED {
            self.do_scroll(ScrollCode::StepBackward, self.track_pos.get());
            return false;
        }
        if self.part_states[2].get() == PartState::PRESSED {
            self.do_scroll(ScrollCode::StepForward, self.track_pos.get());
            return false;
        }
        if self.part_states[3].get() == PartState::PRESSED {
            self.do_scroll(ScrollCode::PageBackward, self.track_pos.get());
            return self.thumb_rect.get().contains_point(self.track_point.get());
        }
        if self.part_states[4].get() == PartState::PRESSED {
            self.do_scroll(ScrollCode::PageForward, self.track_pos.get());
            return self.thumb_rect.get().contains_point(self.track_point.get());
        }
        let wheel = self.mouse_wheel.get();
        if wheel != 0 {
            self.do_scroll(
                if wheel < 0 {
                    ScrollCode::StepForward
                } else {
                    ScrollCode::StepBackward
                },
                self.track_pos.get(),
            );
        }
        true
    }
    fn on_scroll(&self, code: ScrollCode, pos: f32) -> bool {
        if self.base.is_destroyed() {
            return false;
        }
        let mut event = FwlEvent::scroll(self.base.self_ref(), code, pos);
        self.base.dispatch_event(&mut event);
        true
    }
    fn item_rects(&self) -> [CfxRectF; 5] {
        [
            self.min_btn_rect.get(),
            self.thumb_rect.get(),
            self.max_btn_rect.get(),
            self.min_track_rect.get(),
            self.max_track_rect.get(),
        ]
    }
    fn on_lbutton_down(&self, point: CfxPointF) {
        if !self.base.is_enabled() {
            return;
        }
        self.mouse_down.set(true);
        self.base.set_grab(true);
        self.track_point.set(point);
        self.last_track_pos.set(self.track_pos.get());
        let rects = self.item_rects();
        let item = if rects[0].contains_point(point) {
            0
        } else if rects[1].contains_point(point) {
            1
        } else if rects[2].contains_point(point) {
            2
        } else if rects[3].contains_point(point) {
            3
        } else {
            4
        };
        self.do_mouse_down(item, rects[item], point);
        if !self.send_event() && self.mouse_down.get() && !self.base.is_destroyed() {
            self.start_timer(SCROLLBAR_ELAPSED_MSECS);
        }
    }
    fn on_lbutton_up(&self, point: CfxPointF) {
        self.stop_timer();
        self.mouse_down.set(false);
        for (item, rect) in self.item_rects().into_iter().enumerate() {
            self.do_mouse_up(item, rect, point);
        }
        self.base.set_grab(false);
    }
    fn on_mouse_move(&self, point: CfxPointF) {
        for (item, rect) in self.item_rects().into_iter().enumerate() {
            self.do_mouse_move(item, rect, point);
        }
    }
    fn on_mouse_leave(&self) {
        for (item, rect) in self.item_rects().into_iter().enumerate() {
            if self.part_states[item].replace(PartState::NORMAL) != PartState::NORMAL {
                self.base.repaint_rect(rect);
            }
        }
    }
    fn on_mouse_wheel(&self, delta: CfxVector) {
        if !self.base.is_enabled() {
            return;
        }
        self.mouse_wheel.set(delta.y);
        self.send_event();
        self.mouse_wheel.set(0);
    }
    fn do_mouse_down(&self, item: usize, rect: CfxRectF, point: CfxPointF) {
        if !rect.contains_point(point) || self.part_states[item].get() == PartState::PRESSED {
            return;
        }
        self.part_states[item].set(PartState::PRESSED);
        self.base.repaint_rect(rect);
    }
    fn do_mouse_up(&self, item: usize, rect: CfxRectF, point: CfxPointF) {
        let state = if rect.contains_point(point) {
            PartState::HOVERED
        } else {
            PartState::NORMAL
        };
        if self.part_states[item].replace(state) == state {
            return;
        }
        self.base.repaint_rect(rect);
        self.on_scroll(ScrollCode::EndScroll, self.track_pos.get());
    }
    fn do_mouse_move(&self, item: usize, rect: CfxRectF, point: CfxPointF) {
        if !self.mouse_down.get() {
            let state = if rect.contains_point(point) {
                PartState::HOVERED
            } else {
                PartState::NORMAL
            };
            if self.part_states[item].replace(state) != state {
                self.base.repaint_rect(rect);
            }
        } else if item == 2 && self.part_states[1].get() == PartState::PRESSED {
            self.track_pos.set(self.get_track_point_pos(point));
            self.on_scroll(ScrollCode::TrackPos, self.track_pos.get());
        }
    }
    fn stop_timer(&self) {
        self.timer_generation
            .set(self.timer_generation.get().wrapping_add(1));
        let old = self.timer.borrow_mut().take();
        drop(old);
    }
    fn start_timer(&self, interval: i32) {
        self.stop_timer();
        if !self.mouse_down.get() || self.base.is_destroyed() {
            return;
        }
        let generation = self.timer_generation.get();
        let Some(app) = self.base.app() else {
            return;
        };
        let Some(callback) = self
            .weak_self
            .upgrade()
            .map(|owner| owner as Rc<dyn TimerCallback>)
        else {
            return;
        };
        let handler = app.timer_handler();
        let timer = CfxTimer::new(handler.as_ref(), &callback, interval);
        // A host may synchronously hide/destroy/release the control while
        // scheduling. Do not resurrect a cancelled repeat timer afterwards.
        if generation != self.timer_generation.get()
            || !self.mouse_down.get()
            || self.base.is_destroyed()
        {
            timer.cancel();
            return;
        }
        self.timer.replace(Some(timer));
    }
}
impl FwlWidget for CfwlScrollBar {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::ScrollBar
    }
    fn update(&self) {
        if !self.base.is_locked() {
            self.layout();
        }
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if self.base.has_border() {
            self.base.draw_border(graphics, Part::Border, matrix);
        }
        let states = &self.part_states;
        self.draw_part(
            graphics,
            matrix,
            Part::LowerTrack,
            self.min_track_rect.get(),
            states[3].get(),
            false,
        );
        self.draw_part(
            graphics,
            matrix,
            Part::UpperTrack,
            self.max_track_rect.get(),
            states[4].get(),
            false,
        );
        self.draw_part(
            graphics,
            matrix,
            Part::ForeArrow,
            self.min_btn_rect.get(),
            states[0].get(),
            true,
        );
        self.draw_part(
            graphics,
            matrix,
            Part::BackArrow,
            self.max_btn_rect.get(),
            states[2].get(),
            true,
        );
        self.draw_part(
            graphics,
            matrix,
            Part::Thumb,
            self.thumb_rect.get(),
            states[1].get(),
            false,
        );
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        match message.data {
            MessageData::Mouse { command, pos, .. } => match command {
                MouseCommand::LeftButtonDown => self.on_lbutton_down(pos),
                MouseCommand::LeftButtonUp => self.on_lbutton_up(pos),
                MouseCommand::Move => self.on_mouse_move(pos),
                MouseCommand::Leave => self.on_mouse_leave(),
                _ => {}
            },
            MessageData::MouseWheel { delta, .. } => self.on_mouse_wheel(delta),
            _ => {}
        }
    }
    fn set_states(&self, states: WidgetState) {
        self.base.set_states(states);
        if !self.base.is_enabled() || !self.base.is_visible() {
            self.mouse_down.set(false);
            self.stop_timer();
            self.base.set_grab(false);
        }
    }
    fn pre_finalize(&self) {
        self.mouse_down.set(false);
        self.stop_timer();
        self.base.pre_finalize();
    }
}

impl TimerCallback for CfwlScrollBar {
    fn on_timer_fired(&self) {
        self.stop_timer();
        if !self.mouse_down.get()
            || !self.base.is_enabled()
            || !self.base.is_visible()
            || self.base.is_destroyed()
        {
            return;
        }
        if !self.send_event() && self.mouse_down.get() && !self.base.is_destroyed() {
            self.start_timer(0);
        }
    }
}

#[cfg(test)]
#[path = "cfwl_scrollbar_tests.rs"]
mod tests;
