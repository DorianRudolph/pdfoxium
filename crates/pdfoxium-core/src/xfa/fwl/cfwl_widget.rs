// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! FWL widget properties and source common behavior. Small state uses Cells;
//! manager/delegate owners are released before host callbacks and reentry.
use super::{
    cfwl_app::CfwlApp,
    cfwl_event::{EventData, FwlEvent},
    cfwl_message::{FwlMessage, MessageData},
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::Part,
    cfwl_themetext::CfwlThemeText,
    fwl_widgethit::FwlWidgetHit,
    ifwl_widgetdelegate::WidgetDelegate,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, CfxSizeF},
    xfa::{
        fde::cfde_data::{FdeTextAlignment, FdeTextStyle},
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
    },
};
use std::{
    cell::{Cell, RefCell},
    ops::{BitAnd, BitOr, BitOrAssign, Not},
    rc::{Rc, Weak},
};
pub type WidgetId = u64;
pub type WidgetRef = Rc<dyn FwlWidget>;
pub type WidgetWeak = Weak<dyn FwlWidget>;

macro_rules! flags {
    ($name:ident) => {
        impl $name {
            pub const NONE: Self = Self(0);
            pub fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }
            pub fn intersects(self, other: Self) -> bool {
                self.0 & other.0 != 0
            }
            pub fn is_empty(self) -> bool {
                self.0 == 0
            }
        }
        impl BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }
        impl BitAnd for $name {
            type Output = Self;
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }
        impl BitOrAssign for $name {
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }
        impl Not for $name {
            type Output = Self;
            fn not(self) -> Self {
                Self(!self.0)
            }
        }
    };
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WidgetStyle(pub u32);
flags!(WidgetStyle);
impl WidgetStyle {
    pub const OVER_LAPPER: Self = Self(0);
    pub const OVERLAPPER: Self = Self(0);
    pub const POPUP: Self = Self(1);
    pub const CHILD: Self = Self(2);
    pub const BORDER: Self = Self(1 << 2);
    pub const VSCROLL: Self = Self(1 << 11);
    pub const GROUP: Self = Self(1 << 22);
    pub const NO_BACKGROUND: Self = Self(1 << 28);
    pub const WINDOW_TYPE_MASK: Self = Self(3);
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WidgetState(pub u32);
flags!(WidgetState);
impl WidgetState {
    pub const CARET_HIGHLIGHT: Self = Self(1);
    pub const DISABLED: Self = Self(1 << 2);
    pub const FOCUSED: Self = Self(1 << 4);
    pub const INVISIBLE: Self = Self(1 << 5);
    pub const HOVERED: Self = Self(1 << 6);
    pub const PRESSED: Self = Self(1 << 7);
    pub const CHECKBOX_CHECKED: Self = Self(1 << 8);
    pub const CHECKBOX_NEUTRAL: Self = Self(2 << 8);
    pub const CHECKBOX_CHECK_MASK: Self = Self(3 << 8);
    pub const PUSHBUTTON_DEFAULT: Self = Self(1 << 8);
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum FwlType {
    #[default]
    Unknown = 0,
    Barcode,
    Caret,
    CheckBox,
    ComboBox,
    DateTimePicker,
    Edit,
    Form,
    FormProxy,
    ListBox,
    MonthCalendar,
    PictureBox,
    PushButton,
    ScrollBar,
    SpinButton,
    ToolTip,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Properties {
    pub styles: WidgetStyle,
    pub style_exts: u32,
    pub states: WidgetState,
}
impl Default for Properties {
    fn default() -> Self {
        Self {
            styles: WidgetStyle::CHILD,
            style_exts: 0,
            states: WidgetState::NONE,
        }
    }
}

pub trait WidgetAdapter {
    fn xfa_widget(&self) -> Option<Rc<dyn crate::xfa::fxfa::cxfa_ffwidget::FfWidget>> {
        None
    }
    fn get_rotate_matrix(&self) -> CfxMatrix;
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>);
    fn get_border_color_and_thickness(&self) -> (u32, f32);
}
/// Every concrete widget implements its source update/drawing behavior.
/// Default common callbacks and geometry are exactly the base widget's behavior.
pub trait FwlWidget {
    fn as_edit(&self) -> Option<&super::cfwl_edit::CfwlEdit> {
        None
    }
    fn base(&self) -> &CfwlWidget;
    fn get_class_id(&self) -> FwlType;
    /// Composite controls draw some children in their own source draw path.
    /// The manager still visits those children's descendants.
    fn draws_child(&self, _child_id: WidgetId) -> bool {
        false
    }
    /// Checked counterpart to the source's composite-owner casts.
    fn as_combo_box(&self) -> Option<&super::cfwl_combobox::CfwlComboBox> {
        None
    }
    fn as_datetime_picker(&self) -> Option<&super::cfwl_datetimepicker::CfwlDateTimePicker> {
        None
    }
    fn update(&self);
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix);
    fn on_process_message(&self, message: &mut FwlMessage) {
        self.base().on_process_message(message);
    }
    fn on_process_event(&self, _event: &mut FwlEvent) {}
    fn on_draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        self.draw_widget(graphics, matrix);
    }
    fn get_widget_rect(&self) -> CfxRectF {
        self.base().get_widget_rect()
    }
    fn get_client_rect(&self) -> CfxRectF {
        self.base().get_client_rect()
    }
    fn get_autosized_widget_rect(&self) -> CfxRectF {
        CfxRectF::default()
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        self.base().hit_test(point)
    }
    fn set_states(&self, states: WidgetState) {
        self.base().set_states(states);
    }
    fn clear_states(&self, states: WidgetState) {
        self.base().clear_states(states);
    }
    fn modify_style_exts(&self, added: u32, removed: u32) {
        self.base().modify_style_exts(added, removed);
    }
    fn is_form(&self) -> bool {
        false
    }
    fn pre_finalize(&self) {
        self.base().pre_finalize();
    }
}
pub struct CfwlWidget {
    id: WidgetId,
    properties: Cell<Properties>,
    widget_rect: Cell<CfxRectF>,
    app: Weak<CfwlApp>,
    outer: Option<WidgetId>,
    lock: Cell<u32>,
    event_key: Cell<u64>,
    adapter: RefCell<Option<Rc<dyn WidgetAdapter>>>,
    delegate: RefCell<Option<Rc<dyn WidgetDelegate>>>,
    destroyed: Cell<bool>,
}
impl CfwlWidget {
    pub fn new(app: &Rc<CfwlApp>, properties: Properties, outer: Option<WidgetRef>) -> Self {
        Self {
            id: app.allocate_widget_id(),
            properties: Cell::new(properties),
            widget_rect: Cell::new(CfxRectF::default()),
            app: Rc::downgrade(app),
            outer: outer.as_ref().map(|w| w.base().id()),
            lock: Cell::new(0),
            event_key: Cell::new(0),
            adapter: RefCell::new(None),
            delegate: RefCell::new(None),
            destroyed: Cell::new(false),
        }
    }
    pub fn id(&self) -> WidgetId {
        self.id
    }
    pub fn app(&self) -> Option<Rc<CfwlApp>> {
        self.app.upgrade()
    }
    pub fn outer_id(&self) -> Option<WidgetId> {
        self.outer
    }
    pub fn self_ref(&self) -> Option<WidgetRef> {
        self.app()?.widget_mgr().get_widget(self.id)
    }
    pub fn is_registered(&self) -> bool {
        self.self_ref().is_some()
    }
    pub(crate) fn is_finalized(&self) -> bool {
        self.destroyed.get()
    }
    pub fn is_destroyed(&self) -> bool {
        self.destroyed.get() || self.app.strong_count() == 0
    }
    pub fn outer(&self) -> Option<WidgetRef> {
        self.app()?.widget_mgr().get_widget(self.outer?)
    }
    pub fn get_outer(&self) -> Option<WidgetRef> {
        self.outer()
    }
    pub fn get_outmost(&self) -> Option<WidgetRef> {
        let mut widget = self.self_ref()?;
        while let Some(outer) = widget.base().outer() {
            widget = outer;
        }
        Some(widget)
    }
    pub fn pre_finalize(&self) {
        if self.destroyed.replace(true) {
            return;
        }
        debug_assert!(!self.is_locked());
        if let Some(app) = self.app() {
            if let Some(widget) = app.widget_mgr().get_widget(self.id) {
                app.note_driver().notify_target_destroy(&widget);
            }
            app.widget_mgr().remove_widget(self.id);
        }
    }
    pub fn get_event_key(&self) -> u64 {
        self.event_key.get()
    }
    pub fn set_event_key(&self, key: u64) {
        self.event_key.set(key);
    }
    pub fn get_widget_rect(&self) -> CfxRectF {
        self.widget_rect.get()
    }
    pub fn set_widget_rect(&self, rect: CfxRectF) {
        self.widget_rect.set(rect);
    }
    pub fn get_styles(&self) -> WidgetStyle {
        self.properties.get().styles
    }
    pub fn get_style_exts(&self) -> u32 {
        self.properties.get().style_exts
    }
    pub fn get_states(&self) -> WidgetState {
        self.properties.get().states
    }
    pub fn set_styles(&self, styles: WidgetStyle) {
        let mut props = self.properties.get();
        props.styles = styles;
        self.properties.set(props);
    }
    pub fn modify_style_exts(&self, added: u32, removed: u32) {
        let mut props = self.properties.get();
        props.style_exts = (props.style_exts & !removed) | added;
        self.properties.set(props);
    }
    pub fn set_states(&self, states: WidgetState) {
        self.replace_states(self.get_states() | states);
    }
    pub fn replace_states(&self, states: WidgetState) {
        let mut props = self.properties.get();
        props.states = states;
        self.properties.set(props);
        if self.is_visible() {
            return;
        }
        if let Some(app) = self.app() {
            if let Some(widget) = self.self_ref() {
                app.note_driver().notify_target_hide(&widget);
            }
            for child in app.widget_mgr().descendants(self.id) {
                app.note_driver().notify_target_hide(&child);
            }
        }
    }
    pub fn clear_states(&self, states: WidgetState) {
        let mut props = self.properties.get();
        props.states = props.states & !states;
        self.properties.set(props);
    }
    pub fn is_visible(&self) -> bool {
        !self.get_states().intersects(WidgetState::INVISIBLE)
    }
    pub fn is_enabled(&self) -> bool {
        !self.get_states().intersects(WidgetState::DISABLED)
    }
    pub fn is_locked(&self) -> bool {
        self.lock.get() > 0
    }
    pub fn has_border(&self) -> bool {
        self.get_styles().intersects(WidgetStyle::BORDER)
    }
    pub fn is_over_lapper(&self) -> bool {
        (self.get_styles() & WidgetStyle::WINDOW_TYPE_MASK).is_empty()
    }
    pub fn is_popup(&self) -> bool {
        self.get_styles().intersects(WidgetStyle::POPUP)
    }
    pub fn is_child(&self) -> bool {
        self.get_styles().intersects(WidgetStyle::CHILD)
    }
    pub fn get_cx_border_size(&self) -> f32 {
        self.app()
            .map_or(0., |app| app.theme_provider().get_cx_border_size())
    }
    pub fn get_cy_border_size(&self) -> f32 {
        self.app()
            .map_or(0., |app| app.theme_provider().get_cy_border_size())
    }
    pub fn get_relative_rect(&self) -> CfxRectF {
        let rect = self.widget_rect.get();
        CfxRectF::new(0., 0., rect.width, rect.height)
    }
    pub fn get_edge_rect(&self) -> CfxRectF {
        let mut rect = self.get_relative_rect();
        if self.has_border() {
            rect.deflate_xy(self.get_cx_border_size(), self.get_cy_border_size());
        }
        rect
    }
    pub fn get_client_rect(&self) -> CfxRectF {
        self.get_edge_rect()
    }
    pub fn inflate_widget_rect(&self, rect: &mut CfxRectF) {
        if self.has_border() {
            let width = self.get_cx_border_size();
            rect.inflate_xy(width, width);
        }
    }
    pub fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        if self.get_client_rect().contains_point(point) {
            FwlWidgetHit::Client
        } else if self.has_border() && self.get_relative_rect().contains_point(point) {
            FwlWidgetHit::Border
        } else {
            FwlWidgetHit::Unknown
        }
    }
    pub fn get_parent(&self) -> Option<WidgetRef> {
        self.app()?.widget_mgr().get_parent_widget(self.id)
    }
    fn offset_to_root(&self) -> CfxPointF {
        let rect = self.get_widget_rect();
        let mut point = rect.top_left();
        let mut parent = self.get_parent();
        while let Some(widget) = parent {
            let rect = widget.get_widget_rect();
            point.x += rect.left;
            point.y += rect.top;
            parent = widget.base().get_parent();
        }
        point
    }
    pub fn transform_to(&self, target: &WidgetRef, point: CfxPointF) -> CfxPointF {
        let source = self.offset_to_root();
        let destination = target.base().offset_to_root();
        CfxPointF::new(
            point.x + source.x - destination.x,
            point.y + source.y - destination.y,
        )
    }
    pub fn get_matrix(&self) -> CfxMatrix {
        let mut parents = Vec::new();
        let mut parent = self.get_parent();
        while let Some(widget) = parent {
            parent = widget.base().get_parent();
            parents.push(widget);
        }
        let mut matrix = CfxMatrix::default();
        for widget in parents.iter().take(parents.len().saturating_sub(1)).rev() {
            let rect = widget.get_widget_rect();
            matrix.translate_prepend(rect.left, rect.top);
        }
        matrix
    }
    pub fn set_delegate(&self, delegate: Option<Rc<dyn WidgetDelegate>>) {
        let old = self.delegate.replace(delegate);
        drop(old);
    }
    pub fn get_delegate(&self) -> Option<Rc<dyn WidgetDelegate>> {
        self.delegate.borrow().clone()
    }
    pub fn set_adapter_iface(&self, adapter: Option<Rc<dyn WidgetAdapter>>) {
        let old = self.adapter.replace(adapter);
        drop(old);
    }
    pub fn adapter_iface(&self) -> Option<Rc<dyn WidgetAdapter>> {
        self.adapter.borrow().clone()
    }
    pub fn border_color_and_thickness(&self) -> Option<(u32, f32)> {
        Some(
            self.get_outmost()?
                .base()
                .adapter_iface()?
                .get_border_color_and_thickness(),
        )
    }
    pub fn scoped_update_lock(&self) -> ScopedUpdateLock<'_> {
        self.lock.set(self.lock.get().saturating_add(1));
        ScopedUpdateLock(self)
    }
    pub fn set_grab(&self, set: bool) {
        if let Some(app) = self.app() {
            app.note_driver()
                .set_grab(if set { self.self_ref() } else { None });
        }
    }
    pub fn unregister_event_target(&self) {
        if let (Some(app), Some(widget)) = (self.app(), self.self_ref()) {
            app.note_driver().unregister_event_target(&widget);
        }
    }
    pub fn dispatch_event(&self, event: &mut FwlEvent) {
        if let Some(outer) = self.outer() {
            dispatch_event_to(&outer, event);
        } else if let Some(app) = self.app() {
            app.note_driver().send_event(event);
        }
    }
    pub fn repaint_rect(&self, rect: CfxRectF) {
        if let (Some(app), Some(widget)) = (self.app(), self.self_ref()) {
            app.widget_mgr().repaint_widget(&widget, rect);
        }
    }
    pub fn calc_text_size(&self, text: &[u32], multiline: bool) -> CfxSizeF {
        let mut rect = CfxRectF::new(0., 0., if multiline { 120. } else { 2048. }, 2048.);
        let styles = FdeTextStyle {
            line_wrap: multiline,
            single_line: !multiline,
            last_line_height: false,
        };
        self.calc_text_rect(text, styles, FdeTextAlignment::TopLeft, &mut rect);
        rect.size()
    }
    pub fn calc_text_rect(
        &self,
        text: &[u32],
        styles: FdeTextStyle,
        align: FdeTextAlignment,
        rect: &mut CfxRectF,
    ) {
        let (Some(app), Some(widget)) = (self.app(), self.self_ref()) else {
            return;
        };
        let mut params = CfwlThemeText::new(Part::None, widget.as_ref());
        params.text = text.to_vec();
        params.tto_styles = styles;
        params.tto_align = align;
        app.theme_provider().calc_text_rect(&params, rect);
    }
    pub fn draw_border(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        part: Part,
        matrix: CfxMatrix,
    ) {
        let (Some(app), Some(widget)) = (self.app(), self.self_ref()) else {
            return;
        };
        let mut params = CfwlThemeBackground::new(part, widget.as_ref());
        params.matrix = matrix;
        params.part_rect = self.get_relative_rect();
        app.theme_provider().draw_background(&params, graphics);
    }
    pub fn on_process_message(&self, message: &mut FwlMessage) {
        if let MessageData::Mouse { command, .. } = message.data {
            if let Some(widget) = message.destination.clone() {
                let mut event = FwlEvent::with_destination(
                    Some(widget.clone()),
                    Some(widget.clone()),
                    EventData::Mouse(command),
                );
                widget.base().dispatch_event(&mut event);
            }
        }
    }
}
pub struct ScopedUpdateLock<'a>(&'a CfwlWidget);
impl Drop for ScopedUpdateLock<'_> {
    fn drop(&mut self) {
        self.0.lock.set(self.0.lock.get().saturating_sub(1));
    }
}

pub fn dispatch_message_to(widget: &WidgetRef, message: &mut FwlMessage) {
    if widget.base().is_destroyed() {
        return;
    }
    if let Some(delegate) = widget.base().get_delegate() {
        delegate.on_process_message(message);
    } else {
        widget.on_process_message(message);
    }
}
pub fn dispatch_event_to(widget: &WidgetRef, event: &mut FwlEvent) {
    if widget.base().is_destroyed() {
        return;
    }
    if let Some(delegate) = widget.base().get_delegate() {
        delegate.on_process_event(event);
    } else {
        widget.on_process_event(event);
    }
}
pub fn dispatch_draw_to(
    widget: &WidgetRef,
    graphics: &mut CfgasGeGraphics<'_, '_, '_>,
    matrix: CfxMatrix,
) {
    if widget.base().is_destroyed() {
        return;
    }
    if let Some(delegate) = widget.base().get_delegate() {
        delegate.on_draw_widget(graphics, matrix);
    } else {
        widget.on_draw_widget(graphics, matrix);
    }
}
