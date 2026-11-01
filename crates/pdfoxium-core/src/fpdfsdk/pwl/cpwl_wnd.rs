// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Portable `fpdfsdk/pwl/cpwl_wnd.{h,cpp}`. A Window composes this base and
//! binds a weak virtual receiver after Rc construction. Children are owned;
//! parent, host and focus/capture references are weak. Explicit destruction
//! invalidates observers even while a callback temporarily retains the Rc.
use super::cpwl_scroll_bar::{CpwlScrollBar, PwlScrollInfo};
use super::ipwl_fillernotify::{CursorStyle, FillerNotify, PerWindowData};
use super::pwl_draw::PwlDraw;
use crate::fpdfdoc::ipvt_fontmap::IpvtFontMap;
use crate::fpdfsdk::cpdfsdk_baannot::BorderStyle;
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::cfx_timer::TimerHandler;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxge::cfx_color::CfxColor;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

pub const WINDOW_BORDER: u32 = 0x40000000;
pub const WINDOW_BACKGROUND: u32 = 0x20000000;
pub const WINDOW_VSCROLL: u32 = 0x08000000;
pub const WINDOW_VISIBLE: u32 = 0x04000000;
pub const WINDOW_READ_ONLY: u32 = 0x01000000;
pub const WINDOW_AUTO_FONT_SIZE: u32 = 0x00800000;
pub const WINDOW_AUTO_TRANSPARENT: u32 = 0x00400000;
pub const WINDOW_NO_REFRESH_CLIP: u32 = 0x00200000;
pub const EDIT_MULTILINE: u32 = 0x0001;
pub const EDIT_PASSWORD: u32 = 0x0002;
pub const EDIT_LEFT: u32 = 0x0004;
pub const EDIT_RIGHT: u32 = 0x0008;
pub const EDIT_MIDDLE: u32 = 0x0010;
pub const EDIT_TOP: u32 = 0x0020;
pub const EDIT_CENTER: u32 = 0x0080;
pub const EDIT_CHAR_ARRAY: u32 = 0x0100;
pub const EDIT_AUTO_SCROLL: u32 = 0x0200;
pub const EDIT_AUTO_RETURN: u32 = 0x0400;
pub const EDIT_UNDO: u32 = 0x0800;
pub const EDIT_RICH: u32 = 0x1000;
pub const EDIT_TEXT_OVERFLOW: u32 = 0x4000;
pub const LISTBOX_MULTIPLE_SEL: u32 = 0x0001;
pub const LISTBOX_HOVER_SEL: u32 = 0x0008;
pub const COMBOBOX_ALLOW_CUSTOM_TEXT: u32 = 0x0001;
const WINDOW_STYLE_MASK: u32 = WINDOW_BORDER
    | WINDOW_BACKGROUND
    | WINDOW_VSCROLL
    | WINDOW_VISIBLE
    | WINDOW_READ_ONLY
    | WINDOW_AUTO_FONT_SIZE
    | WINDOW_AUTO_TRANSPARENT
    | WINDOW_NO_REFRESH_CLIP;
pub const DEFAULT_BLACK_COLOR: CfxColor = CfxColor::make_gray(0.0);
pub const DEFAULT_WHITE_COLOR: CfxColor = CfxColor::make_gray(1.0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dash {
    pub dash: i32,
    pub gap: i32,
    pub phase: i32,
}
impl Default for Dash {
    fn default() -> Self {
        Self {
            dash: 3,
            gap: 0,
            phase: 0,
        }
    }
}
pub trait ProviderIface {
    fn window_matrix(&self, data: Option<&dyn PerWindowData>) -> CfxMatrix;
    fn on_set_focus_for_edit(&self, edit: &dyn Window);
}
#[derive(Clone)]
pub struct CreateParams {
    pub rect: CfxFloatRect,
    pub timer_handler: Option<Weak<dyn TimerHandler>>,
    pub filler_notify: Option<Weak<dyn FillerNotify>>,
    pub provider: Option<Weak<dyn ProviderIface>>,
    pub font_map: Option<Rc<dyn IpvtFontMap>>,
    pub flags: u32,
    pub background_color: CfxColor,
    pub border_style: BorderStyle,
    pub border_width: i32,
    pub border_color: CfxColor,
    pub text_color: CfxColor,
    pub transparency: i32,
    pub font_size: f32,
    pub dash: Dash,
    pub cursor_type: CursorStyle,
    pub shared_state: Option<Rc<SharedCaptureFocusState>>,
}
impl Default for CreateParams {
    fn default() -> Self {
        Self {
            rect: CfxFloatRect::default(),
            timer_handler: None,
            filler_notify: None,
            provider: None,
            font_map: None,
            flags: 0,
            background_color: CfxColor::default(),
            border_style: BorderStyle::Solid,
            border_width: 1,
            border_color: CfxColor::default(),
            text_color: CfxColor::default(),
            transparency: 255,
            font_size: 9.0,
            dash: Dash::default(),
            cursor_type: CursorStyle::Arrow,
            shared_state: None,
        }
    }
}
impl CreateParams {
    pub fn new(
        timer_handler: Option<&Rc<dyn TimerHandler>>,
        filler_notify: Option<&Rc<dyn FillerNotify>>,
        provider: Option<&Rc<dyn ProviderIface>>,
    ) -> Self {
        Self {
            timer_handler: timer_handler.map(Rc::downgrade),
            filler_notify: filler_notify.map(Rc::downgrade),
            provider: provider.map(Rc::downgrade),
            ..Self::default()
        }
    }
}

/// Virtual entry points have the same default behavior as CPWL_Wnd. Concrete
/// controls override only their upstream overrides; base handlers stay callable
/// through `wnd().base_*` without dispatching recursively to the override.
pub trait Window: Any {
    fn as_any(&self) -> &dyn Any;
    fn wnd(&self) -> &CpwlWnd;
    fn on_key_down(&self, key: i32, flags: u32) -> bool {
        self.wnd().base_on_key_down(key, flags)
    }
    fn on_char(&self, ch: u16, flags: u32) -> bool {
        self.wnd().base_on_char(ch, flags)
    }
    fn on_l_button_dbl_clk(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd().base_on_l_button_dbl_clk(flags, point)
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd().base_on_l_button_down(flags, point)
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd().base_on_l_button_up(flags, point)
    }
    fn on_r_button_down(&self, _flags: u32, _point: CfxPointF) -> bool {
        false
    }
    fn on_r_button_up(&self, _flags: u32, _point: CfxPointF) -> bool {
        false
    }
    fn on_mouse_move(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd().base_on_mouse_move(flags, point)
    }
    fn on_mouse_wheel(&self, flags: u32, point: CfxPointF, delta: CfxPointF) -> bool {
        self.wnd().base_on_mouse_wheel(flags, point, delta)
    }
    fn set_scroll_info(&self, _info: &PwlScrollInfo) {}
    fn set_scroll_position(&self, _pos: f32) {}
    fn scroll_window_vertically(&self, _pos: f32) {}
    fn scroll_bar_width(&self) -> f32 {
        0.0
    }
    fn notify_l_button_down(&self, _child: &dyn Window, _pos: CfxPointF) {}
    fn notify_l_button_up(&self, _child: &dyn Window, _pos: CfxPointF) {}
    fn notify_mouse_move(&self, _child: &dyn Window, _pos: CfxPointF) {}
    fn set_focus(&self) {
        self.wnd().set_focus();
    }
    fn kill_focus(&self) {
        self.wnd().kill_focus();
    }
    fn set_cursor(&self) {
        self.wnd().set_cursor();
    }
    fn set_visible(&self, visible: bool) -> bool {
        self.wnd().set_visible(visible)
    }
    fn invalidate_rect(&self, rect: Option<CfxFloatRect>) -> bool {
        self.wnd().invalidate_rect(rect)
    }
    fn set_font_size(&self, size: f32) {
        self.wnd().set_font_size(size);
    }
    fn font_size(&self) -> f32 {
        self.wnd().font_size()
    }
    fn set_text_direction(&self, _direction: ParagraphDirection) {}
    fn text(&self) -> Vec<u32> {
        Vec::new()
    }
    fn selected_text(&self) -> Vec<u32> {
        Vec::new()
    }
    fn get_text(&self) -> Vec<u32> {
        self.text()
    }
    fn get_selected_text(&self) -> Vec<u32> {
        self.selected_text()
    }
    fn replace_and_keep_selection(&self, _text: &[u32]) {}
    fn replace_selection(&self, _text: &[u32]) {}
    fn select_all_text(&self) -> bool {
        false
    }
    fn can_undo(&self) -> bool {
        false
    }
    fn can_redo(&self) -> bool {
        false
    }
    fn undo(&self) -> bool {
        false
    }
    fn redo(&self) -> bool {
        false
    }
    fn focus_rect(&self) -> CfxFloatRect {
        self.wnd().focus_rect()
    }
    fn client_rect(&self) -> CfxFloatRect {
        self.wnd().client_rect()
    }
    fn on_set_focus(&self) {}
    fn on_kill_focus(&self) {}
    fn create_child_wnd(&self, _cp: &CreateParams) {}
    fn reposition_child_wnd(&self) -> bool {
        self.wnd().base_reposition_child_wnd()
    }
    fn draw_this_appearance(&self, device: &mut dyn PwlDraw, matrix: CfxMatrix) {
        self.wnd().base_draw_this_appearance(device, matrix);
    }
    fn on_created(&self) {}
    fn on_destroy(&self) {}
    fn draw_appearance(&self, device: &mut dyn PwlDraw, matrix: CfxMatrix) {
        self.wnd().draw_appearance(device, matrix);
    }
}

pub struct SharedCaptureFocusState {
    active: Cell<bool>,
    mouse_paths: RefCell<Vec<Weak<dyn Window>>>,
    keyboard_paths: RefCell<Vec<Weak<dyn Window>>>,
    main_keyboard: RefCell<Option<Weak<dyn Window>>>,
}
impl SharedCaptureFocusState {
    fn new() -> Self {
        Self {
            active: Cell::new(true),
            mouse_paths: RefCell::new(Vec::new()),
            keyboard_paths: RefCell::new(Vec::new()),
            main_keyboard: RefCell::new(None),
        }
    }
    fn contains(paths: &RefCell<Vec<Weak<dyn Window>>>, id: u64) -> bool {
        paths.borrow().iter().any(|w| {
            w.upgrade()
                .is_some_and(|w| w.wnd().id == id && w.wnd().is_alive())
        })
    }
    fn main_is(&self, id: u64) -> bool {
        self.main_keyboard
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|w| w.wnd().id == id && w.wnd().is_alive())
    }
    fn remove(&self, id: u64) {
        for paths in [&self.mouse_paths, &self.keyboard_paths] {
            paths
                .borrow_mut()
                .retain(|w| w.upgrade().is_some_and(|w| w.wnd().id != id));
        }
        if self
            .main_keyboard
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|w| w.wnd().id == id)
        {
            self.main_keyboard.borrow_mut().take();
        }
    }
    fn release_focus(&self) {
        // Clear storage before calling the old control. Reentrant focus changes
        // then keep their new target rather than recursively killing the old one.
        let old = self
            .keyboard_paths
            .borrow_mut()
            .drain(..)
            .next()
            .and_then(|w| w.upgrade());
        self.main_keyboard.borrow_mut().take();
        if let Some(old) = old {
            old.on_kill_focus();
        }
    }
}
thread_local! {static NEXT_ID:Cell<u64>=const {Cell::new(1)};}
pub struct CpwlWnd {
    id: u64,
    creation_params: RefCell<CreateParams>,
    attached_data: Option<Box<dyn PerWindowData>>,
    owner: RefCell<Option<Weak<dyn Window>>>,
    parent: RefCell<Option<Weak<dyn Window>>>,
    children: RefCell<Vec<Rc<dyn Window>>>,
    vscroll_bar: RefCell<Option<Weak<dyn Window>>>,
    window_rect: Cell<CfxFloatRect>,
    clip_rect: Cell<CfxFloatRect>,
    created: Cell<bool>,
    visible: Cell<bool>,
    alive: Cell<bool>,
    destroying: Cell<bool>,
    owns_shared: Cell<bool>,
}
impl CpwlWnd {
    pub fn new(cp: CreateParams, attached_data: Option<Box<dyn PerWindowData>>) -> Self {
        let id = NEXT_ID.with(|id| {
            let current = id.get();
            id.set(current.checked_add(1).expect("window identity exhausted"));
            current
        });
        Self {
            id,
            creation_params: RefCell::new(cp),
            attached_data,
            owner: RefCell::new(None),
            parent: RefCell::new(None),
            children: RefCell::new(Vec::new()),
            vscroll_bar: RefCell::new(None),
            window_rect: Cell::new(CfxFloatRect::default()),
            clip_rect: Cell::new(CfxFloatRect::default()),
            created: Cell::new(false),
            visible: Cell::new(false),
            alive: Cell::new(true),
            destroying: Cell::new(false),
            owns_shared: Cell::new(false),
        }
    }
    pub fn bind_owner(&self, owner: &Rc<dyn Window>) {
        assert_eq!(owner.wnd().id, self.id);
        *self.owner.borrow_mut() = Some(Rc::downgrade(owner));
    }
    pub fn owner(&self) -> Option<Rc<dyn Window>> {
        self.owner.borrow().as_ref().and_then(Weak::upgrade)
    }
    pub fn is_alive(&self) -> bool {
        self.alive.get() && !self.destroying.get()
    }
    pub fn is_valid(&self) -> bool {
        self.is_alive() && self.created.get()
    }
    pub fn creation_params(&self) -> CreateParams {
        self.creation_params.borrow().clone()
    }
    pub fn update_creation_params<R>(&self, f: impl FnOnce(&mut CreateParams) -> R) -> R {
        f(&mut self.creation_params.borrow_mut())
    }
    pub fn attached_data(&self) -> Option<&dyn PerWindowData> {
        self.attached_data.as_deref()
    }
    pub fn clone_attached_data(&self) -> Option<Box<dyn PerWindowData>> {
        self.attached_data().map(PerWindowData::clone_data)
    }
    pub fn filler_notify(&self) -> Option<Rc<dyn FillerNotify>> {
        self.creation_params
            .borrow()
            .filler_notify
            .as_ref()
            .and_then(Weak::upgrade)
    }
    pub fn provider(&self) -> Option<Rc<dyn ProviderIface>> {
        self.creation_params
            .borrow()
            .provider
            .as_ref()
            .and_then(Weak::upgrade)
    }
    pub fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        self.creation_params
            .borrow()
            .timer_handler
            .as_ref()
            .and_then(Weak::upgrade)
    }
    pub fn font_map(&self) -> Option<Rc<dyn IpvtFontMap>> {
        self.creation_params.borrow().font_map.clone()
    }
    pub fn invalidate_provider(&self, provider: &Rc<dyn ProviderIface>) {
        self.update_creation_params(|cp| {
            if cp
                .provider
                .as_ref()
                .is_some_and(|p| p.ptr_eq(&Rc::downgrade(provider)))
            {
                cp.provider = None;
            }
        });
    }
    pub fn parent(&self) -> Option<Rc<dyn Window>> {
        self.parent
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .filter(|p| p.wnd().is_alive())
    }
    pub fn children(&self) -> Vec<Rc<dyn Window>> {
        self.children.borrow().clone()
    }
    pub fn add_child(&self, child: Rc<dyn Window>) {
        if !self.is_alive() {
            child.wnd().destroy();
            return;
        }
        assert!(child.wnd().is_alive());
        assert!(
            !self
                .ancestors()
                .iter()
                .any(|ancestor| ancestor.wnd().id == child.wnd().id),
            "window tree cycle"
        );
        if child.wnd().creation_params.borrow().shared_state.is_none() {
            child.wnd().creation_params.borrow_mut().shared_state = self.shared();
        }
        assert!(child
            .wnd()
            .parent
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .is_none());
        let owner = self.owner().expect("bind window before adding children");
        *child.wnd().parent.borrow_mut() = Some(Rc::downgrade(&owner));
        self.children.borrow_mut().push(child);
    }
    pub fn remove_child(&self, child: &dyn Window) {
        let removed = {
            let mut children = self.children.borrow_mut();
            children
                .iter()
                .position(|item| item.wnd().id == child.wnd().id)
                .map(|index| children.remove(index))
        };
        if child
            .wnd()
            .parent
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|parent| parent.wnd().id == self.id)
        {
            child.wnd().parent.borrow_mut().take();
        }
        drop(removed);
    }
    fn shared(&self) -> Option<Rc<SharedCaptureFocusState>> {
        self.creation_params
            .borrow()
            .shared_state
            .clone()
            .filter(|s| s.active.get())
    }
    pub fn realize(&self) {
        assert!(!self.created.get());
        if !self.is_alive() {
            return;
        }
        let owner = self.owner().expect("bind window before realizing it");
        let mut cp = self.creation_params();
        cp.rect.normalize();
        self.window_rect.set(cp.rect);
        let mut clip = cp.rect;
        if !clip.is_empty() {
            clip.inflate_xy(1.0, 1.0);
            clip.normalize();
        }
        self.clip_rect.set(clip);
        if cp.shared_state.is_none() {
            cp.shared_state = Some(Rc::new(SharedCaptureFocusState::new()));
            self.owns_shared.set(true);
        }
        *self.creation_params.borrow_mut() = cp.clone();
        cp.flags &= WINDOW_STYLE_MASK;
        self.create_vscroll_bar(&cp);
        if !self.is_alive() {
            return;
        }
        owner.create_child_wnd(&cp);
        if !self.is_alive() {
            return;
        }
        self.visible.set(self.has_flag(WINDOW_VISIBLE));
        owner.on_created();
        if !self.is_alive() || !owner.reposition_child_wnd() || !self.is_alive() {
            return;
        }
        self.created.set(true);
    }
    pub fn destroy(&self) {
        if !self.alive.get() || self.destroying.replace(true) {
            return;
        }
        if let Some(owner) = self.owner() {
            owner.kill_focus();
            owner.on_destroy();
        }
        self.vscroll_bar.borrow_mut().take();
        loop {
            let child = self.children.borrow_mut().pop();
            let Some(child) = child else { break };
            child.wnd().destroy();
        }
        let parent = self.parent.borrow_mut().take().and_then(|p| p.upgrade());
        if let Some(parent) = parent {
            if let Some(owner) = self.owner() {
                parent.wnd().remove_child(owner.as_ref());
            }
        }
        self.created.set(false);
        if let Some(shared) = self.shared() {
            shared.remove(self.id);
            if self.owns_shared.get() {
                shared.active.set(false);
                shared.mouse_paths.borrow_mut().clear();
                shared.keyboard_paths.borrow_mut().clear();
                shared.main_keyboard.borrow_mut().take();
            }
        }
        self.creation_params.borrow_mut().shared_state = None;
        self.alive.set(false);
        self.destroying.set(false);
    }
    pub fn move_window(&self, rect: CfxFloatRect, reset: bool, refresh: bool) -> bool {
        if !self.is_valid() {
            return self.is_alive();
        }
        let old = self.window_rect();
        let mut normalized = rect;
        normalized.normalize();
        self.window_rect.set(normalized);
        if reset && old != rect {
            if let Some(owner) = self.owner() {
                if !owner.reposition_child_wnd() || !self.is_alive() {
                    return false;
                }
            }
        }
        if refresh && !self.invalidate_rect_move(old, rect) {
            return false;
        }
        self.creation_params.borrow_mut().rect = self.window_rect();
        self.is_alive()
    }
    pub fn invalidate_rect_move(&self, old: CfxFloatRect, new: CfxFloatRect) -> bool {
        let mut union = old;
        union.union(new);
        self.owner().map_or_else(
            || self.invalidate_rect(Some(union)),
            |owner| owner.invalidate_rect(Some(union)),
        )
    }
    pub fn invalidate_rect(&self, rect: Option<CfxFloatRect>) -> bool {
        if !self.is_valid() {
            return self.is_alive();
        }
        let mut refresh = rect.unwrap_or_else(|| self.window_rect());
        if !self.has_flag(WINDOW_NO_REFRESH_CLIP) {
            let clip = self.clip_rect();
            if !clip.is_empty() {
                refresh.intersect(clip);
            }
        }
        let mut win = self.window_matrix().transform_float_rect(refresh);
        if !self.is_alive() {
            return false;
        }
        win.inflate_xy(1.0, 1.0);
        win.normalize();
        if let Some(notify) = self.filler_notify() {
            notify.invalidate_rect(self.attached_data(), win);
        }
        self.is_alive()
    }
    pub fn base_on_key_down(&self, key: i32, flags: u32) -> bool {
        if !self.is_valid() || !self.is_visible() || !self.is_wnd_capture_keyboard_id(self.id) {
            return false;
        }
        for child in self.children() {
            if self.is_wnd_capture_keyboard_id(child.wnd().id) {
                return child.on_key_down(key, flags);
            }
        }
        false
    }
    pub fn base_on_char(&self, ch: u16, flags: u32) -> bool {
        if !self.is_valid() || !self.is_visible() || !self.is_wnd_capture_keyboard_id(self.id) {
            return false;
        }
        for child in self.children() {
            if self.is_wnd_capture_keyboard_id(child.wnd().id) {
                return child.on_char(ch, flags);
            }
        }
        false
    }
    fn mouse(
        &self,
        flags: u32,
        point: CfxPointF,
        dispatch: fn(&dyn Window, u32, CfxPointF) -> bool,
    ) -> bool {
        if !self.is_valid() || !self.is_visible() {
            return false;
        }
        if self.is_capture_mouse() {
            for child in self.children() {
                if self.is_wnd_capture_mouse_id(child.wnd().id) {
                    return dispatch(child.as_ref(), flags, point);
                }
            }
            self.dispatch_cursor();
            return false;
        }
        for child in self.children() {
            if child.wnd().wnd_hit_test(point) {
                return dispatch(child.as_ref(), flags, point);
            }
        }
        if self.wnd_hit_test(point) {
            self.dispatch_cursor();
        }
        false
    }
    pub fn base_on_l_button_dbl_clk(&self, flags: u32, point: CfxPointF) -> bool {
        self.mouse(flags, point, |w, f, p| w.on_l_button_dbl_clk(f, p))
    }
    pub fn base_on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.mouse(flags, point, |w, f, p| w.on_l_button_down(f, p))
    }
    pub fn base_on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.mouse(flags, point, |w, f, p| w.on_l_button_up(f, p))
    }
    pub fn base_on_mouse_move(&self, flags: u32, point: CfxPointF) -> bool {
        self.mouse(flags, point, |w, f, p| w.on_mouse_move(f, p))
    }
    pub fn base_on_mouse_wheel(&self, flags: u32, point: CfxPointF, delta: CfxPointF) -> bool {
        if !self.is_valid() || !self.is_visible() {
            return false;
        }
        self.dispatch_cursor();
        if !self.is_alive() || !self.is_wnd_capture_keyboard_id(self.id) {
            return false;
        }
        for child in self.children() {
            if self.is_wnd_capture_keyboard_id(child.wnd().id) {
                return child.on_mouse_wheel(flags, point, delta);
            }
        }
        false
    }
    fn dispatch_cursor(&self) {
        if let Some(owner) = self.owner() {
            owner.set_cursor();
        }
    }
    pub fn set_cursor(&self) {
        if self.is_valid() {
            if let Some(notify) = self.filler_notify() {
                let cursor = self.creation_params.borrow().cursor_type;
                notify.set_cursor(cursor);
            }
        }
    }
    pub fn set_cursor_type(&self, cursor: CursorStyle) {
        self.creation_params.borrow_mut().cursor_type = cursor;
    }
    pub fn window_rect(&self) -> CfxFloatRect {
        self.window_rect.get()
    }
    pub fn client_rect(&self) -> CfxFloatRect {
        let window = self.window_rect();
        let width = (self.border_width() + self.inner_border_width()) as f32;
        let mut client = window.get_deflated(width, width);
        if let Some(bar) = self.v_scroll_bar() {
            client.right -= bar.scroll_bar_width();
        }
        client.normalize();
        if window.contains_rect(client) {
            client
        } else {
            CfxFloatRect::default()
        }
    }
    pub fn base_client_rect(&self) -> CfxFloatRect {
        self.client_rect()
    }
    pub fn center_point(&self) -> CfxPointF {
        let rect = self
            .owner()
            .map_or_else(|| self.client_rect(), |w| w.client_rect());
        CfxPointF::new(
            (rect.left + rect.right) * 0.5,
            (rect.bottom + rect.top) * 0.5,
        )
    }
    pub fn has_flag(&self, flag: u32) -> bool {
        self.creation_params.borrow().flags & flag != 0
    }
    pub fn remove_flag(&self, flag: u32) {
        self.creation_params.borrow_mut().flags &= !flag;
    }
    pub fn border_width(&self) -> i32 {
        if self.has_flag(WINDOW_BORDER) {
            self.creation_params.borrow().border_width
        } else {
            0
        }
    }
    pub fn inner_border_width(&self) -> i32 {
        0
    }
    pub fn background_color(&self) -> CfxColor {
        self.creation_params.borrow().background_color
    }
    pub fn text_color(&self) -> CfxColor {
        self.creation_params.borrow().text_color
    }
    pub fn border_color(&self) -> CfxColor {
        if self.has_flag(WINDOW_BORDER) {
            self.creation_params.borrow().border_color
        } else {
            CfxColor::default()
        }
    }
    pub fn border_style(&self) -> BorderStyle {
        self.creation_params.borrow().border_style
    }
    pub fn border_dash(&self) -> Dash {
        self.creation_params.borrow().dash
    }
    pub fn transparency(&self) -> i32 {
        self.creation_params.borrow().transparency
    }
    pub fn set_transparency(&self, alpha: i32) {
        for child in self.children() {
            child.wnd().set_transparency(alpha);
        }
        self.creation_params.borrow_mut().transparency = alpha;
    }
    pub fn font_size(&self) -> f32 {
        self.creation_params.borrow().font_size
    }
    pub fn set_font_size(&self, size: f32) {
        self.creation_params.borrow_mut().font_size = size;
    }
    pub fn border_left_top_color(&self, style: BorderStyle) -> CfxColor {
        match style {
            BorderStyle::Beveled => DEFAULT_WHITE_COLOR,
            BorderStyle::Inset => CfxColor::make_gray(0.5),
            _ => CfxColor::default(),
        }
    }
    pub fn border_right_bottom_color(&self, style: BorderStyle) -> CfxColor {
        match style {
            BorderStyle::Beveled => self.background_color() / 2.0,
            BorderStyle::Inset => CfxColor::make_gray(0.75),
            _ => CfxColor::default(),
        }
    }
    pub fn v_scroll_bar(&self) -> Option<Rc<dyn Window>> {
        if !self.has_flag(WINDOW_VSCROLL) {
            return None;
        }
        self.vscroll_bar
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .filter(|bar| bar.wnd().is_alive())
    }
    fn create_vscroll_bar(&self, cp: &CreateParams) {
        if self.v_scroll_bar().is_some() || !self.has_flag(WINDOW_VSCROLL) {
            return;
        }
        let mut scp = cp.clone();
        scp.flags = WINDOW_BACKGROUND | WINDOW_AUTO_TRANSPARENT | WINDOW_NO_REFRESH_CLIP;
        scp.background_color = DEFAULT_WHITE_COLOR;
        scp.cursor_type = CursorStyle::Arrow;
        scp.transparency = CpwlScrollBar::TRANSPARENCY as i32;
        let bar = CpwlScrollBar::new(scp, self.clone_attached_data());
        let bar: Rc<dyn Window> = bar;
        *self.vscroll_bar.borrow_mut() = Some(Rc::downgrade(&bar));
        self.add_child(Rc::clone(&bar));
        bar.wnd().realize();
    }
    pub fn base_reposition_child_wnd(&self) -> bool {
        let Some(bar) = self.v_scroll_bar() else {
            return self.is_alive();
        };
        let mut content = self.window_rect();
        if !content.is_empty() {
            let width = (self.border_width() + self.inner_border_width()) as f32;
            content.deflate_xy(width, width);
            content.normalize();
        }
        bar.wnd().move_window(
            CfxFloatRect::new(
                content.right - CpwlScrollBar::WIDTH,
                content.bottom,
                content.right - 1.0,
                content.top,
            ),
            true,
            false,
        );
        self.is_alive()
    }
    pub fn set_capture(&self) {
        if !self.is_alive() {
            return;
        }
        if let Some(shared) = self.shared() {
            *shared.mouse_paths.borrow_mut() = self.ancestors().iter().map(Rc::downgrade).collect();
        }
    }
    pub fn release_capture(&self) {
        for child in self.children() {
            child.wnd().release_capture();
        }
        if let Some(shared) = self.shared() {
            shared.mouse_paths.borrow_mut().clear();
        }
    }
    pub fn ancestors(&self) -> Vec<Rc<dyn Window>> {
        let mut result = Vec::new();
        let mut current = self.owner();
        while let Some(window) = current {
            current = window.wnd().parent();
            result.push(window);
        }
        result
    }
    pub fn set_focus(&self) {
        if !self.is_alive() {
            return;
        }
        let Some(shared) = self.shared() else {
            return;
        };
        if !shared.main_is(self.id) {
            shared.release_focus();
        }
        if !self.is_alive() || !shared.active.get() {
            return;
        }
        let Some(owner) = self.owner() else {
            return;
        };
        *shared.keyboard_paths.borrow_mut() = self.ancestors().iter().map(Rc::downgrade).collect();
        *shared.main_keyboard.borrow_mut() = Some(Rc::downgrade(&owner));
        owner.on_set_focus();
    }
    pub fn kill_focus(&self) {
        if let Some(shared) = self.shared() {
            let in_path = shared
                .keyboard_paths
                .borrow()
                .iter()
                .any(|w| w.upgrade().is_some_and(|w| w.wnd().id == self.id));
            if in_path {
                shared.release_focus();
            }
        }
    }
    fn is_wnd_capture_mouse_id(&self, id: u64) -> bool {
        self.shared()
            .is_some_and(|s| SharedCaptureFocusState::contains(&s.mouse_paths, id))
    }
    fn is_wnd_capture_keyboard_id(&self, id: u64) -> bool {
        self.shared()
            .is_some_and(|s| SharedCaptureFocusState::contains(&s.keyboard_paths, id))
    }
    pub fn is_wnd_capture_mouse(&self, wnd: &dyn Window) -> bool {
        self.is_wnd_capture_mouse_id(wnd.wnd().id)
    }
    pub fn is_wnd_capture_keyboard(&self, wnd: &dyn Window) -> bool {
        self.is_wnd_capture_keyboard_id(wnd.wnd().id)
    }
    pub fn is_capture_mouse(&self) -> bool {
        self.is_wnd_capture_mouse_id(self.id)
    }
    pub fn is_focused(&self) -> bool {
        self.shared().is_some_and(|s| s.main_is(self.id))
    }
    pub fn wnd_hit_test(&self, point: CfxPointF) -> bool {
        self.is_valid() && self.is_visible() && self.window_rect().contains_point(point)
    }
    pub fn client_hit_test(&self, point: CfxPointF) -> bool {
        self.is_valid()
            && self.is_visible()
            && self
                .owner()
                .map_or_else(|| self.client_rect(), |w| w.client_rect())
                .contains_point(point)
    }
    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }
    pub fn is_read_only(&self) -> bool {
        self.has_flag(WINDOW_READ_ONLY)
    }
    pub fn set_visible(&self, visible: bool) -> bool {
        if !self.is_valid() {
            return self.is_alive();
        }
        for child in self.children() {
            if !child.set_visible(visible) || !self.is_alive() {
                return false;
            }
        }
        if visible != self.visible.replace(visible) {
            if let Some(owner) = self.owner() {
                if !owner.reposition_child_wnd() || !self.is_alive() {
                    return false;
                }
            }
            if !self.owner().map_or_else(
                || self.invalidate_rect(None),
                |owner| owner.invalidate_rect(None),
            ) {
                return false;
            }
        }
        self.is_alive()
    }
    pub fn set_clip_rect(&self, mut rect: CfxFloatRect) {
        rect.normalize();
        self.clip_rect.set(rect);
    }
    pub fn clip_rect(&self) -> CfxFloatRect {
        self.clip_rect.get()
    }
    pub fn focus_rect(&self) -> CfxFloatRect {
        let mut rect = self.window_rect();
        if !rect.is_empty() {
            rect.inflate_xy(1.0, 1.0);
            rect.normalize();
        }
        rect
    }
    pub fn window_matrix(&self) -> CfxMatrix {
        self.provider().map_or_else(CfxMatrix::default, |provider| {
            provider.window_matrix(self.attached_data())
        })
    }
    pub fn draw_appearance(&self, device: &mut dyn PwlDraw, matrix: CfxMatrix) {
        if !self.is_valid() || !self.is_visible() {
            return;
        }
        if let Some(owner) = self.owner() {
            owner.draw_this_appearance(device, matrix);
        }
        if !self.is_alive() {
            return;
        }
        for child in self.children() {
            child.draw_appearance(device, matrix);
            if !self.is_alive() {
                return;
            }
        }
    }
    pub fn base_draw_this_appearance(&self, device: &mut dyn PwlDraw, matrix: CfxMatrix) {
        let rect = self.window_rect();
        if rect.is_empty() {
            return;
        }
        if self.has_flag(WINDOW_BACKGROUND) {
            let width = (self.border_width() + self.inner_border_width()) as f32;
            device.fill_rect(
                matrix,
                rect.get_deflated(width, width),
                self.background_color().to_fx_color(self.transparency()),
            );
        }
        if self.has_flag(WINDOW_BORDER) {
            let style = self.border_style();
            device.draw_border(
                matrix,
                rect,
                self.border_width() as f32,
                self.border_color(),
                self.border_left_top_color(style),
                self.border_right_bottom_color(style),
                style,
                self.transparency(),
            );
        }
    }
}
impl Drop for CpwlWnd {
    fn drop(&mut self) {
        if let Some(shared) = self.creation_params.get_mut().shared_state.as_ref() {
            shared.remove(self.id);
            if self.owns_shared.get() {
                shared.active.set(false);
            }
        }
    }
}

pub struct PlainWindow {
    wnd: CpwlWnd,
}
impl PlainWindow {
    pub fn new(cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        let window = Rc::new(Self {
            wnd: CpwlWnd::new(cp, attached),
        });
        let owner: Rc<dyn Window> = window.clone();
        window.wnd.bind_owner(&owner);
        window
    }
}
impl Window for PlainWindow {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
}

pub fn is_shift_key_down(flags: u32) -> bool {
    flags & 1 != 0
}
pub fn is_ctrl_key_down(flags: u32) -> bool {
    flags & 2 != 0
}
pub fn is_alt_key_down(flags: u32) -> bool {
    flags & 4 != 0
}
pub fn is_meta_key_down(flags: u32) -> bool {
    flags & 8 != 0
}
pub fn is_platform_shortcut_key(flags: u32) -> bool {
    if cfg!(target_vendor = "apple") {
        is_meta_key_down(flags)
    } else {
        is_ctrl_key_down(flags)
    }
}

#[cfg(test)]
mod tests {
    use super::super::ipwl_fillernotify::BeforeKeystrokeResult;
    use super::*;
    #[derive(Default)]
    struct Host {
        invalidated: RefCell<Vec<CfxFloatRect>>,
        target: RefCell<Option<Weak<dyn Window>>>,
        destroy_on_invalidate: Cell<bool>,
        cursor: Cell<CursorStyle>,
    }
    impl FillerNotify for Host {
        fn invalidate_rect(&self, _data: Option<&dyn PerWindowData>, rect: CfxFloatRect) {
            self.invalidated.borrow_mut().push(rect);
            if self.destroy_on_invalidate.get() {
                let target = self.target.borrow().as_ref().and_then(Weak::upgrade);
                if let Some(target) = target {
                    target.wnd().destroy();
                }
            }
        }
        fn output_selected_rect(&self, _data: Option<&dyn PerWindowData>, _rect: CfxFloatRect) {}
        fn is_selection_implemented(&self) -> bool {
            false
        }
        fn set_cursor(&self, cursor: CursorStyle) {
            self.cursor.set(cursor);
        }
        fn query_where_popup(
            &self,
            _data: Option<&dyn PerWindowData>,
            _min: f32,
            max: f32,
        ) -> (bool, f32) {
            (true, max)
        }
        fn on_before_key_stroke(
            &self,
            _data: Option<&dyn PerWindowData>,
            _change: &mut Vec<u32>,
            _ex: &[u32],
            _start: i32,
            _end: i32,
            _down: bool,
            _flags: u32,
        ) -> BeforeKeystrokeResult {
            BeforeKeystrokeResult {
                rc: true,
                exit: false,
            }
        }
        fn on_popup_pre_open(&self, _data: Option<&dyn PerWindowData>, _flags: u32) -> bool {
            true
        }
        fn on_popup_post_open(&self, _data: Option<&dyn PerWindowData>, _flags: u32) -> bool {
            true
        }
    }
    struct Provider;
    impl ProviderIface for Provider {
        fn window_matrix(&self, _data: Option<&dyn PerWindowData>) -> CfxMatrix {
            CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 10.0, 20.0)
        }
        fn on_set_focus_for_edit(&self, _edit: &dyn Window) {}
    }
    fn params() -> CreateParams {
        CreateParams {
            rect: CfxFloatRect::new(0.0, 0.0, 100.0, 100.0),
            flags: WINDOW_VISIBLE,
            ..CreateParams::default()
        }
    }
    #[test]
    fn geometry_styles_and_transformed_invalidation_follow_the_source_order() {
        let host = Rc::new(Host::default());
        let notify: Rc<dyn FillerNotify> = host.clone();
        let provider: Rc<dyn ProviderIface> = Rc::new(Provider);
        let mut cp = CreateParams::new(None, Some(&notify), Some(&provider));
        cp.rect = CfxFloatRect::new(90.0, 80.0, 10.0, 20.0);
        cp.flags = WINDOW_VISIBLE | WINDOW_BORDER | WINDOW_BACKGROUND;
        cp.border_width = 2;
        cp.border_style = BorderStyle::Beveled;
        cp.background_color = CfxColor::make_gray(0.8);
        let window = PlainWindow::new(cp, None);
        window.wnd.realize();
        assert!(window.wnd.is_valid());
        assert_eq!(
            window.wnd.window_rect(),
            CfxFloatRect::new(10.0, 20.0, 90.0, 80.0)
        );
        assert_eq!(
            window.wnd.client_rect(),
            CfxFloatRect::new(12.0, 22.0, 88.0, 78.0)
        );
        assert_eq!(
            window.wnd.focus_rect(),
            CfxFloatRect::new(9.0, 19.0, 91.0, 81.0)
        );
        assert_eq!(
            window.wnd.border_left_top_color(BorderStyle::Beveled),
            DEFAULT_WHITE_COLOR
        );
        assert_eq!(
            window.wnd.border_right_bottom_color(BorderStyle::Beveled),
            CfxColor::make_gray(0.4)
        );
        assert!(window
            .wnd
            .invalidate_rect(Some(CfxFloatRect::new(-100.0, -100.0, 200.0, 200.0))));
        assert_eq!(
            &*host.invalidated.borrow(),
            &[CfxFloatRect::new(18.0, 38.0, 102.0, 102.0)]
        );
        window.wnd.destroy();
        assert!(!window.wnd.is_alive());
        assert!(!window.wnd.is_valid());
    }
    #[test]
    fn invalidation_can_destroy_the_tree_without_retained_borrows_or_live_observers() {
        let host = Rc::new(Host::default());
        let notify: Rc<dyn FillerNotify> = host.clone();
        let mut cp = params();
        cp.filler_notify = Some(Rc::downgrade(&notify));
        let parent = PlainWindow::new(cp, None);
        parent.wnd.realize();
        let child = PlainWindow::new(parent.wnd.creation_params(), None);
        let dyn_child: Rc<dyn Window> = child.clone();
        parent.wnd.add_child(dyn_child.clone());
        child.wnd.realize();
        child.wnd.set_capture();
        child.wnd.set_focus();
        assert!(child.wnd.is_focused());
        assert!(parent.wnd.is_capture_mouse());
        let dyn_parent: Rc<dyn Window> = parent.clone();
        *host.target.borrow_mut() = Some(Rc::downgrade(&dyn_parent));
        host.destroy_on_invalidate.set(true);
        assert!(!child.wnd.invalidate_rect(None));
        assert!(!child.wnd.is_alive());
        assert!(!parent.wnd.is_alive());
        assert!(!child.wnd.is_focused());
        assert!(!child.wnd.is_capture_mouse());
        assert!(parent.wnd.children().is_empty());
        assert!(!child.wnd.set_visible(true));
        assert!(!child
            .wnd
            .move_window(CfxFloatRect::new(0.0, 0.0, 2.0, 2.0), true, true));
        assert!(child.wnd.parent().is_none());
    }
    struct InputWindow {
        wnd: CpwlWnd,
        clicks: Cell<u32>,
        keys: Cell<u32>,
        focuses: Cell<u32>,
        kills: Cell<u32>,
        kill_target: RefCell<Option<Weak<dyn Window>>>,
    }
    impl InputWindow {
        fn new(cp: CreateParams) -> Rc<Self> {
            let window = Rc::new(Self {
                wnd: CpwlWnd::new(cp, None),
                clicks: Cell::new(0),
                keys: Cell::new(0),
                focuses: Cell::new(0),
                kills: Cell::new(0),
                kill_target: RefCell::new(None),
            });
            let owner: Rc<dyn Window> = window.clone();
            window.wnd.bind_owner(&owner);
            window
        }
    }
    impl Window for InputWindow {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn wnd(&self) -> &CpwlWnd {
            &self.wnd
        }
        fn on_l_button_down(&self, _flags: u32, _point: CfxPointF) -> bool {
            self.clicks.set(self.clicks.get() + 1);
            true
        }
        fn on_key_down(&self, _key: i32, _flags: u32) -> bool {
            self.keys.set(self.keys.get() + 1);
            true
        }
        fn on_set_focus(&self) {
            self.focuses.set(self.focuses.get() + 1);
        }
        fn on_kill_focus(&self) {
            self.kills.set(self.kills.get() + 1);
            let target = self.kill_target.borrow().as_ref().and_then(Weak::upgrade);
            if let Some(target) = target {
                target.wnd().destroy();
            }
        }
    }
    #[test]
    fn focus_and_capture_route_through_ancestors_and_survive_destruction_in_focus_callback() {
        let parent = PlainWindow::new(params(), None);
        parent.wnd.realize();
        let mut cp = parent.wnd.creation_params();
        cp.rect = CfxFloatRect::new(10.0, 10.0, 20.0, 20.0);
        let first = InputWindow::new(cp.clone());
        let first_dyn: Rc<dyn Window> = first.clone();
        parent.wnd.add_child(first_dyn.clone());
        first.wnd.realize();
        let second = InputWindow::new(cp);
        let second_dyn: Rc<dyn Window> = second.clone();
        parent.wnd.add_child(second_dyn.clone());
        second.wnd.realize();
        assert!(parent.on_l_button_down(0, CfxPointF::new(15.0, 15.0)));
        assert_eq!(first.clicks.get(), 1);
        assert_eq!(second.clicks.get(), 0);
        second.wnd.set_capture();
        assert!(parent.on_l_button_down(0, CfxPointF::new(500.0, 500.0)));
        assert_eq!(second.clicks.get(), 1);
        second.wnd.set_focus();
        assert!(parent.on_key_down(65, 0));
        assert_eq!(second.keys.get(), 1);
        assert!(!first.wnd.is_focused());
        let parent_dyn: Rc<dyn Window> = parent.clone();
        *second.kill_target.borrow_mut() = Some(Rc::downgrade(&parent_dyn));
        first.wnd.set_focus();
        assert_eq!(second.kills.get(), 1);
        assert!(!parent.wnd.is_alive());
        assert!(!first.wnd.is_alive());
        assert_eq!(first.focuses.get(), 0);
    }
    #[test]
    fn host_and_parent_links_are_weak_and_invalidated_by_drop() {
        let host = Rc::new(Host::default());
        let notify: Rc<dyn FillerNotify> = host.clone();
        let mut cp = params();
        cp.filler_notify = Some(Rc::downgrade(&notify));
        let window = PlainWindow::new(cp, None);
        window.wnd.realize();
        drop(notify);
        drop(host);
        assert!(window.wnd.filler_notify().is_none());
        assert!(window.wnd.invalidate_rect(None));
        let child = PlainWindow::new(window.wnd.creation_params(), None);
        let dyn_child: Rc<dyn Window> = child.clone();
        window.wnd.add_child(dyn_child);
        child.wnd.realize();
        drop(window);
        assert!(child.wnd.parent().is_none());
        assert!(!child.wnd.is_focused());
        child.wnd.destroy();
        assert!(is_shift_key_down(1));
        assert!(is_ctrl_key_down(2));
        assert!(is_alt_key_down(4));
        assert!(is_meta_key_down(8));
        assert!(!is_platform_shortcut_key(0));
    }
}
