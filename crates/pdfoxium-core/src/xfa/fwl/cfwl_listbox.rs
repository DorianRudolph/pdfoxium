// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/cfwl_listbox.{h,cpp}` port. Item owners remain stable across
//! vector growth/removal; collection borrows never span theme or host callbacks.
use super::{
    cfwl_app::CfwlApp,
    cfwl_event::{EventData, FwlEvent, ScrollCode},
    cfwl_message::{FwlMessage, KeyCommand, MessageData, MouseCommand},
    cfwl_scrollbar::{CfwlScrollBar, FWL_STYLEEXT_SCB_HORZ, FWL_STYLEEXT_SCB_VERT},
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{CfwlThemePart, Part, PartState},
    cfwl_themetext::CfwlThemeText,
    cfwl_widget::{
        dispatch_message_to, CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef, WidgetState,
        WidgetStyle,
    },
    fwl_widgetdef::{
        KEY_FLAG_CTRL, KEY_FLAG_SHIFT, VKEY_DOWN, VKEY_END, VKEY_HOME, VKEY_TAB, VKEY_UP,
    },
    fwl_widgethit::FwlWidgetHit,
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
    rc::{Rc, Weak},
};

pub const FWL_STYLEEXT_LTB_MULTI_SELECTION: u32 = 1;
pub const FWL_STYLEEXT_LTB_LEFT_ALIGN: u32 = 0;
pub const FWL_STYLEEXT_LTB_CENTER_ALIGN: u32 = 1 << 4;
pub const FWL_STYLEEXT_LTB_RIGHT_ALIGN: u32 = 2 << 4;
pub const FWL_STYLEEXT_LTB_ALIGN_MASK: u32 = 3 << 4;
pub const FWL_STYLEEXT_LTB_SHOW_SCROLL_BAR_FOCUS: u32 = 1 << 10;
const ITEM_TEXT_MARGIN: f32 = 2.;

#[derive(Debug)]
pub struct ListItem {
    selected: Cell<bool>,
    focused: Cell<bool>,
    rect: Cell<CfxRectF>,
    text: Vec<u32>,
}
impl ListItem {
    pub fn new(text: &[u32]) -> Self {
        Self {
            selected: Cell::new(false),
            focused: Cell::new(false),
            rect: Cell::new(CfxRectF::default()),
            text: text.to_vec(),
        }
    }
    pub fn is_selected(&self) -> bool {
        self.selected.get()
    }
    pub fn set_selected(&self, selected: bool) {
        self.selected.set(selected);
    }
    pub fn is_focused(&self) -> bool {
        self.focused.get()
    }
    pub fn set_focused(&self, focused: bool) {
        self.focused.set(focused);
    }
    pub fn get_rect(&self) -> CfxRectF {
        self.rect.get()
    }
    pub fn set_rect(&self, rect: CfxRectF) {
        self.rect.set(rect);
    }
    pub fn get_text(&self) -> &[u32] {
        &self.text
    }
    pub fn text(&self) -> &[u32] {
        &self.text
    }
}
pub type Item = ListItem;
pub struct CfwlListBox {
    base: CfwlWidget,
    client_rect: Cell<CfxRectF>,
    static_rect: Cell<CfxRectF>,
    content_rect: Cell<CfxRectF>,
    horizontal_scroll_bar: RefCell<Option<Rc<CfwlScrollBar>>>,
    vertical_scroll_bar: RefCell<Option<Rc<CfwlScrollBar>>>,
    text_styles: Cell<FdeTextStyle>,
    text_alignment: Cell<FdeTextAlignment>,
    left_button_down: Cell<bool>,
    item_height: Cell<f32>,
    scroll_bar_width: Cell<f32>,
    items: RefCell<Vec<Rc<ListItem>>>,
    anchor: RefCell<Option<Weak<ListItem>>>,
}
impl CfwlListBox {
    pub fn new(app: &Rc<CfwlApp>, properties: Properties, outer: Option<WidgetRef>) -> Rc<Self> {
        let list = Rc::new(Self::new_unregistered(app, properties, outer));
        app.register_widget(list.clone());
        list
    }
    /// Composition subclasses register their wrapper once, retaining this base.
    pub fn new_unregistered(
        app: &Rc<CfwlApp>,
        properties: Properties,
        outer: Option<WidgetRef>,
    ) -> Self {
        Self {
            base: CfwlWidget::new(app, properties, outer),
            client_rect: Cell::new(CfxRectF::default()),
            static_rect: Cell::new(CfxRectF::default()),
            content_rect: Cell::new(CfxRectF::default()),
            horizontal_scroll_bar: RefCell::new(None),
            vertical_scroll_bar: RefCell::new(None),
            text_styles: Cell::new(FdeTextStyle::default()),
            text_alignment: Cell::new(FdeTextAlignment::TopLeft),
            left_button_down: Cell::new(false),
            item_height: Cell::new(0.),
            scroll_bar_width: Cell::new(0.),
            items: RefCell::new(Vec::new()),
            anchor: RefCell::new(None),
        }
    }
    pub fn client_rect(&self) -> CfxRectF {
        self.client_rect.get()
    }
    pub fn get_rt_client(&self) -> CfxRectF {
        self.client_rect.get()
    }
    pub fn content_rect(&self) -> CfxRectF {
        self.content_rect.get()
    }
    pub fn get_item_height(&self) -> f32 {
        self.item_height.get()
    }
    pub fn get_vert_scroll_bar(&self) -> Option<Rc<CfwlScrollBar>> {
        self.vertical_scroll_bar.borrow().clone()
    }
    pub fn get_horz_scroll_bar(&self) -> Option<Rc<CfwlScrollBar>> {
        self.horizontal_scroll_bar.borrow().clone()
    }
    pub fn count_items(&self) -> i32 {
        self.items.borrow().len().min(i32::MAX as usize) as i32
    }
    pub fn get_item(&self, index: i32) -> Option<Rc<ListItem>> {
        self.items
            .borrow()
            .get(usize::try_from(index).ok()?)
            .cloned()
    }
    pub fn get_item_index(&self, item: Option<&Rc<ListItem>>) -> i32 {
        let Some(item) = item else {
            return -1;
        };
        self.items
            .borrow()
            .iter()
            .position(|candidate| Rc::ptr_eq(candidate, item))
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
    }
    pub fn add_string(&self, text: &[u32]) -> Rc<ListItem> {
        let item = Rc::new(ListItem::new(text));
        self.items.borrow_mut().push(item.clone());
        item
    }
    pub fn remove_at(&self, index: i32) {
        let Some(index) = usize::try_from(index).ok() else {
            return;
        };
        let removed = {
            let mut items = self.items.borrow_mut();
            if index >= items.len() {
                return;
            }
            items.remove(index)
        };
        let clear = self
            .anchor
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|anchor| Rc::ptr_eq(&anchor, &removed));
        if clear {
            self.anchor.borrow_mut().take();
        }
    }
    pub fn delete_string(&self, item: &Rc<ListItem>) {
        let index = self.get_item_index(Some(item));
        if index < 0 {
            return;
        }
        let next = if index + 1 < self.count_items() {
            index + 1
        } else {
            index - 1
        };
        if let Some(neighbor) = self.get_item(next) {
            if self.is_multi_selection() {
                neighbor.set_selected(true);
            } else {
                self.set_selection(Some(&neighbor), Some(&neighbor), true);
            }
        }
        self.remove_at(index);
    }
    pub fn delete_all(&self) {
        self.items.borrow_mut().clear();
        self.anchor.borrow_mut().take();
    }
    pub fn count_sel_items(&self) -> i32 {
        self.items
            .borrow()
            .iter()
            .filter(|item| item.is_selected())
            .count()
            .min(i32::MAX as usize) as i32
    }
    pub fn get_sel_index(&self, selection: i32) -> i32 {
        let Some(selection) = usize::try_from(selection).ok() else {
            return -1;
        };
        self.items
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_selected())
            .nth(selection)
            .and_then(|(index, _)| i32::try_from(index).ok())
            .unwrap_or(-1)
    }
    pub fn get_sel_item(&self, selection: i32) -> Option<Rc<ListItem>> {
        self.get_item(self.get_sel_index(selection))
    }
    pub fn set_sel_item(&self, item: Option<&Rc<ListItem>>, selected: bool) {
        let Some(item) = item else {
            if selected {
                self.select_all();
            } else {
                self.clear_selection();
                self.set_focus_item(None);
            }
            return;
        };
        if self.get_item_index(Some(item)) < 0 {
            return;
        }
        if self.is_multi_selection() {
            item.set_selected(selected);
        } else {
            self.set_selection(Some(item), Some(item), selected);
        }
    }
    pub fn is_multi_selection(&self) -> bool {
        self.base.get_style_exts() & FWL_STYLEEXT_LTB_MULTI_SELECTION != 0
    }
    pub fn set_selection(
        &self,
        start: Option<&Rc<ListItem>>,
        end: Option<&Rc<ListItem>>,
        selected: bool,
    ) {
        let mut first = self.get_item_index(start);
        let mut last = self.get_item_index(end);
        if first < 0 || last < 0 {
            return;
        }
        if first > last {
            std::mem::swap(&mut first, &mut last);
        }
        let items = self.items.borrow();
        if selected {
            for item in items.iter() {
                item.set_selected(false);
            }
        }
        for item in &items[first as usize..=last as usize] {
            item.set_selected(selected);
        }
    }
    pub fn clear_selection(&self) {
        let multi = self.is_multi_selection();
        for item in self.items.borrow().iter() {
            if item.is_selected() {
                item.set_selected(false);
                if !multi {
                    break;
                }
            }
        }
    }
    pub fn select_all(&self) {
        if !self.is_multi_selection() {
            return;
        }
        // The source passes false here and accidentally clears every item.
        for item in self.items.borrow().iter() {
            item.set_selected(true);
        }
    }
    pub fn get_focused_item(&self) -> Option<Rc<ListItem>> {
        self.items
            .borrow()
            .iter()
            .find(|item| item.is_focused())
            .cloned()
    }
    pub fn set_focus_item(&self, item: Option<&Rc<ListItem>>) {
        if let Some(item) = item {
            if self.get_item_index(Some(item)) < 0 {
                return;
            }
        }
        let focused = self.get_focused_item();
        if let Some(previous) = focused {
            if item.is_some_and(|item| Rc::ptr_eq(item, &previous)) {
                return;
            }
            previous.set_focused(false);
        }
        if let Some(item) = item {
            item.set_focused(true);
        }
    }
    pub fn get_list_item(&self, item: Option<&Rc<ListItem>>, key: u32) -> Option<Rc<ListItem>> {
        let index = match key {
            VKEY_UP => self.get_item_index(item) - 1,
            VKEY_DOWN => self.get_item_index(item) + 1,
            VKEY_HOME => 0,
            VKEY_END => self.count_items() - 1,
            _ => return None,
        };
        self.get_item(index)
    }
    pub fn get_item_at_point(&self, point: CfxPointF) -> Option<Rc<ListItem>> {
        let content = self.content_rect.get();
        let pos = CfxPointF::new(point.x - content.left, point.y - content.top);
        let x = self.get_horz_scroll_bar().map_or(0., |bar| bar.get_pos());
        let y = self.get_vert_scroll_bar().map_or(0., |bar| bar.get_pos());
        for item in self.items.borrow().iter() {
            let mut rect = item.get_rect();
            rect.offset(-x, -y);
            if rect.contains_point(pos) {
                return Some(item.clone());
            }
        }
        None
    }
    pub fn scroll_to_visible(&self, item: Option<&Rc<ListItem>>) -> bool {
        let Some(bar) = self.get_vert_scroll_bar() else {
            return false;
        };
        let content = self.content_rect.get();
        let mut rect = item.map_or(CfxRectF::default(), |item| item.get_rect());
        let mut y = bar.get_pos();
        rect.offset(0., -y + content.top);
        if rect.top < content.top {
            y += rect.top - content.top;
        } else if rect.bottom() > content.bottom() {
            y += rect.bottom() - content.bottom();
        } else {
            return false;
        }
        let (min, max) = bar.get_range();
        y = y.clamp(min, max.max(min));
        bar.set_pos(y);
        bar.set_track_pos(y);
        self.base.repaint_rect(self.client_rect.get());
        true
    }
    pub fn calc_item_height(&self) -> f32 {
        let Some(app) = self.base.app() else {
            return 0.;
        };
        let part = CfwlThemePart::new(Part::None, self);
        app.theme_provider().get_font_size(&part) + 2. * ITEM_TEXT_MARGIN
    }
    fn draw_background(&self, graphics: &mut CfgasGeGraphics, matrix: CfxMatrix) {
        let Some(app) = self.base.app() else {
            return;
        };
        let mut params = CfwlThemeBackground::new(Part::Background, self);
        params.matrix = matrix;
        params.part_rect = self.client_rect.get();
        if self.is_show_horz_scroll_bar() && self.is_show_vert_scroll_bar() {
            params.data_rect = Some(self.static_rect.get());
        }
        if !self.base.is_enabled() {
            params.states = PartState::DISABLED;
        }
        app.theme_provider().draw_background(&params, graphics);
    }
    fn draw_items(&self, graphics: &mut CfgasGeGraphics, matrix: CfxMatrix) {
        let x = self.get_horz_scroll_bar().map_or(0., |bar| bar.get_pos());
        let y = self.get_vert_scroll_bar().map_or(0., |bar| bar.get_pos());
        let content = self.content_rect.get();
        let count = self.count_items();
        for index in 0..count {
            let Some(item) = self.get_item(index) else {
                continue;
            };
            let mut rect = item.get_rect();
            rect.offset(content.left - x, content.top - y);
            if rect.bottom() < content.top {
                continue;
            }
            if rect.top >= content.bottom() {
                break;
            }
            self.draw_item(graphics, &item, rect, matrix);
            if self.base.is_destroyed() {
                break;
            }
        }
    }
    fn draw_item(
        &self,
        graphics: &mut CfgasGeGraphics,
        item: &Rc<ListItem>,
        rect: CfxRectF,
        matrix: CfxMatrix,
    ) {
        let Some(app) = self.base.app() else {
            return;
        };
        let mut states = if !self.base.is_enabled() {
            PartState::DISABLED
        } else if item.is_selected() {
            PartState::SELECTED
        } else {
            PartState::NORMAL
        };
        if self.base.get_states().contains(WidgetState::FOCUSED) && item.is_focused() {
            states |= PartState::FOCUSED;
        }
        let mut focus = rect;
        let mut background = CfwlThemeBackground::new(Part::ListItem, self);
        background.states = states;
        background.matrix = matrix;
        background.part_rect = rect;
        background.maximize = true;
        if self.get_vert_scroll_bar().is_some()
            && self.get_horz_scroll_bar().is_none()
            && states.contains(PartState::FOCUSED)
        {
            background.part_rect.left += 1.;
            background.part_rect.width -= self.scroll_bar_width.get() + 1.;
            focus.deflate(0.5, 0.5, 1. + self.scroll_bar_width.get(), 1.);
        }
        background.data_rect = Some(focus);
        let theme = app.theme_provider();
        theme.draw_background(&background, graphics);
        if self.base.is_destroyed() || self.get_item_index(Some(item)) < 0 || item.text().is_empty()
        {
            return;
        }
        let mut text_rect = rect;
        text_rect.deflate_xy(ITEM_TEXT_MARGIN, ITEM_TEXT_MARGIN);
        let mut text = CfwlThemeText::new(Part::ListItem, self);
        text.states = states;
        text.matrix = matrix;
        text.part_rect = text_rect;
        text.text = item.text().to_vec();
        text.tto_styles = self.text_styles.get();
        text.tto_align = self.text_alignment.get();
        text.maximize = true;
        theme.draw_text(&text, graphics);
    }
    fn get_max_text_width(&self) -> f32 {
        let mut width: f32 = 0.;
        for index in 0..self.count_items() {
            if let Some(item) = self.get_item(index) {
                width = width.max(self.base.calc_text_size(item.text(), false).width);
                if self.base.is_destroyed() {
                    break;
                }
            }
        }
        width
    }
    pub fn calc_size(&self) -> CfxSizeF {
        self.client_rect.set(self.get_client_rect());
        let client = self.client_rect.get();
        let mut content = client;
        let Some(app) = self.base.app() else {
            return CfxSizeF::default();
        };
        if self.base.outer_id().is_none() {
            let part = CfwlThemePart::new(Part::None, self);
            let margin = app.theme_provider().get_ui_margin(&part);
            content.deflate(margin.left, margin.top, margin.width, margin.height);
        }
        self.content_rect.set(content);
        let width = (self.get_max_text_width() + 2. * ITEM_TEXT_MARGIN).max(client.width);
        let height = self.calc_item_height();
        self.item_height.set(height);
        if self.base.is_destroyed() {
            return CfxSizeF::default();
        }
        let mut size = CfxSizeF::new(width, 0.);
        for item in self.items.borrow().iter() {
            item.set_rect(CfxRectF::new(0., size.height, width, height));
            size.height += height;
        }
        let show_vertical =
            self.base.get_styles().contains(WidgetStyle::VSCROLL) && size.height > client.height;
        // No selected source style enables horizontal scrolling, but retain the
        // full horizontal setup branch for the source's existing consumer path.
        let show_horizontal = false;
        if show_vertical {
            self.init_vertical_scroll_bar();
            if let Some(bar) = self.get_vert_scroll_bar() {
                let rect = CfxRectF::new(
                    client.right() - self.scroll_bar_width.get(),
                    client.top,
                    self.scroll_bar_width.get(),
                    (client.height
                        - 1.
                        - if show_horizontal {
                            self.scroll_bar_width.get()
                        } else {
                            0.
                        })
                    .max(0.),
                );
                bar.base().set_widget_rect(rect);
                let max = (size.height - content.height).max(height).max(0.);
                bar.set_range(0., max);
                bar.set_page_size(rect.height * 9. / 10.);
                bar.set_step_size(height);
                let pos = bar.get_pos().clamp(0., max);
                bar.set_pos(pos);
                bar.set_track_pos(pos);
                if self.scroll_bar_properties_present() {
                    bar.clear_states(WidgetState::INVISIBLE);
                } else {
                    bar.set_states(WidgetState::INVISIBLE);
                }
                bar.update();
            }
        } else if let Some(bar) = self.get_vert_scroll_bar() {
            bar.set_pos(0.);
            bar.set_track_pos(0.);
            bar.set_states(WidgetState::INVISIBLE);
        }
        if show_horizontal {
            self.init_horizontal_scroll_bar();
            if let Some(bar) = self.get_horz_scroll_bar() {
                let rect = CfxRectF::new(
                    client.left,
                    client.bottom() - self.scroll_bar_width.get(),
                    (client.width
                        - if show_vertical {
                            self.scroll_bar_width.get()
                        } else {
                            0.
                        })
                    .max(0.),
                    self.scroll_bar_width.get(),
                );
                bar.base().set_widget_rect(rect);
                let max = (size.width - rect.width).max(0.);
                bar.set_range(0., max);
                bar.set_page_size(width * 9. / 10.);
                bar.set_step_size(width / 10.);
                let pos = bar.get_pos().clamp(0., max);
                bar.set_pos(pos);
                bar.set_track_pos(pos);
                if self.scroll_bar_properties_present() {
                    bar.clear_states(WidgetState::INVISIBLE);
                } else {
                    bar.set_states(WidgetState::INVISIBLE);
                }
                bar.update();
            }
        } else if let Some(bar) = self.get_horz_scroll_bar() {
            bar.set_pos(0.);
            bar.set_track_pos(0.);
            bar.set_states(WidgetState::INVISIBLE);
        }
        if show_vertical && show_horizontal {
            self.static_rect.set(CfxRectF::new(
                client.right() - self.scroll_bar_width.get(),
                client.bottom() - self.scroll_bar_width.get(),
                self.scroll_bar_width.get(),
                self.scroll_bar_width.get(),
            ));
        }
        size
    }
    pub fn init_vertical_scroll_bar(&self) {
        self.init_scroll_bar(true);
    }
    pub fn init_horizontal_scroll_bar(&self) {
        self.init_scroll_bar(false);
    }
    fn init_scroll_bar(&self, vertical: bool) {
        let slot = if vertical {
            &self.vertical_scroll_bar
        } else {
            &self.horizontal_scroll_bar
        };
        if slot.borrow().is_some() {
            return;
        }
        let (Some(app), Some(owner)) = (self.base.app(), self.base.self_ref()) else {
            return;
        };
        let bar = CfwlScrollBar::new(
            &app,
            Properties {
                styles: WidgetStyle::NONE,
                style_exts: if vertical {
                    FWL_STYLEEXT_SCB_VERT
                } else {
                    FWL_STYLEEXT_SCB_HORZ
                },
                states: WidgetState::INVISIBLE,
            },
            Some(owner),
        );
        if self.base.is_destroyed() {
            app.remove_widget(bar.base().id());
            return;
        }
        let old = slot.replace(Some(bar));
        drop(old);
    }
    pub fn scroll_bar_properties_present(&self) -> bool {
        self.base.get_style_exts() & FWL_STYLEEXT_LTB_SHOW_SCROLL_BAR_FOCUS == 0
            || self.base.get_states().contains(WidgetState::FOCUSED)
    }
    pub fn is_show_vert_scroll_bar(&self) -> bool {
        self.scroll_bar_properties_present()
            && self
                .get_vert_scroll_bar()
                .is_some_and(|bar| bar.base().is_visible())
    }
    pub fn is_show_horz_scroll_bar(&self) -> bool {
        self.scroll_bar_properties_present()
            && self
                .get_horz_scroll_bar()
                .is_some_and(|bar| bar.base().is_visible())
    }
    fn on_focus(&self, gained: bool) {
        if gained {
            self.base.set_states(WidgetState::FOCUSED);
        } else {
            self.base.clear_states(WidgetState::FOCUSED);
        }
        if self.base.get_style_exts() & FWL_STYLEEXT_LTB_SHOW_SCROLL_BAR_FOCUS != 0 {
            for bar in [self.get_vert_scroll_bar(), self.get_horz_scroll_bar()]
                .into_iter()
                .flatten()
            {
                if gained {
                    bar.clear_states(WidgetState::INVISIBLE);
                } else {
                    bar.set_states(WidgetState::INVISIBLE);
                }
            }
        }
        self.base.repaint_rect(self.client_rect.get());
    }
    fn anchor(&self) -> Option<Rc<ListItem>> {
        self.anchor
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .filter(|item| self.get_item_index(Some(item)) >= 0)
    }
    fn on_left_button_down(&self, point: CfxPointF, flags: u32) {
        self.left_button_down.set(true);
        let Some(item) = self.get_item_at_point(point) else {
            return;
        };
        if self.is_multi_selection() {
            if flags & KEY_FLAG_CTRL != 0 {
                item.set_selected(!item.is_selected());
                *self.anchor.borrow_mut() = Some(Rc::downgrade(&item));
            } else if flags & KEY_FLAG_SHIFT != 0 {
                if let Some(anchor) = self.anchor() {
                    self.set_selection(Some(&anchor), Some(&item), true);
                } else {
                    item.set_selected(true);
                }
            } else {
                self.set_selection(Some(&item), Some(&item), true);
                *self.anchor.borrow_mut() = Some(Rc::downgrade(&item));
            }
        } else {
            self.set_selection(Some(&item), Some(&item), true);
        }
        self.set_focus_item(Some(&item));
        self.scroll_to_visible(Some(&item));
        if self.base.is_destroyed() {
            return;
        }
        self.base.set_grab(true);
        self.base.repaint_rect(self.client_rect.get());
    }
    pub fn on_vk(&self, item: Option<&Rc<ListItem>>, shift: bool, ctrl: bool) {
        let Some(item) = item else {
            return;
        };
        if self.get_item_index(Some(item)) < 0 {
            return;
        }
        if self.is_multi_selection() {
            if ctrl {
            } else if shift {
                if let Some(anchor) = self.anchor() {
                    self.set_selection(Some(&anchor), Some(item), true);
                } else {
                    item.set_selected(true);
                }
            } else {
                self.set_selection(Some(item), Some(item), true);
                *self.anchor.borrow_mut() = Some(Rc::downgrade(item));
            }
        } else {
            self.set_selection(Some(item), Some(item), true);
        }
        self.set_focus_item(Some(item));
        self.scroll_to_visible(Some(item));
        self.base.repaint_rect(self.base.get_relative_rect());
    }
    pub fn on_scroll(&self, bar: &Rc<CfwlScrollBar>, code: ScrollCode, mut pos: f32) -> bool {
        let (min, max) = bar.get_range();
        let current = bar.get_pos();
        let step = bar.get_step_size();
        match code {
            ScrollCode::Min => pos = min,
            ScrollCode::Max => pos = max,
            ScrollCode::StepBackward => {
                pos -= step;
                if pos < min + step / 2. {
                    pos = min;
                }
            }
            ScrollCode::StepForward => {
                pos += step;
                if pos > max - step / 2. {
                    pos = max;
                }
            }
            ScrollCode::PageBackward => pos = (pos - bar.get_page_size()).max(min),
            ScrollCode::PageForward => pos = (pos + bar.get_page_size()).min(max),
            ScrollCode::Pos | ScrollCode::TrackPos | ScrollCode::None => {}
            ScrollCode::EndScroll => return false,
        }
        if !pos.is_finite() {
            return false;
        }
        pos = pos.clamp(min, max.max(min));
        if current != pos {
            bar.set_pos(pos);
            bar.set_track_pos(pos);
            self.base.repaint_rect(self.client_rect.get());
        }
        true
    }
}
impl FwlWidget for CfwlListBox {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::ListBox
    }
    fn update(&self) {
        if self.base.is_locked() || self.base.is_destroyed() {
            return;
        }
        self.text_alignment.set(
            match self.base.get_style_exts() & FWL_STYLEEXT_LTB_ALIGN_MASK {
                FWL_STYLEEXT_LTB_LEFT_ALIGN => FdeTextAlignment::CenterLeft,
                FWL_STYLEEXT_LTB_RIGHT_ALIGN => FdeTextAlignment::CenterRight,
                _ => FdeTextAlignment::Center,
            },
        );
        self.text_styles.set(FdeTextStyle {
            single_line: true,
            ..Default::default()
        });
        if let Some(app) = self.base.app() {
            self.scroll_bar_width
                .set(app.theme_provider().get_scroll_bar_width());
        }
        self.calc_size();
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        if self.is_show_horz_scroll_bar()
            && self
                .get_horz_scroll_bar()
                .is_some_and(|bar| bar.get_widget_rect().contains_point(point))
        {
            FwlWidgetHit::HScrollBar
        } else if self.is_show_vert_scroll_bar()
            && self
                .get_vert_scroll_bar()
                .is_some_and(|bar| bar.get_widget_rect().contains_point(point))
        {
            FwlWidgetHit::VScrollBar
        } else if self.client_rect.get().contains_point(point) {
            FwlWidgetHit::Client
        } else {
            FwlWidgetHit::Unknown
        }
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics, matrix: CfxMatrix) {
        if self.base.is_destroyed() {
            return;
        }
        let mut graphics = graphics.state_restorer();
        if self.base.has_border() {
            self.base.draw_border(&mut graphics, Part::Border, matrix);
        }
        let mut clip = self.content_rect.get();
        if self.is_show_horz_scroll_bar() {
            clip.height -= self.scroll_bar_width.get();
        }
        if self.is_show_vert_scroll_bar() {
            clip.width -= self.scroll_bar_width.get();
        }
        graphics.set_clip_rect(matrix.transform_rect(clip));
        if !self.base.get_styles().contains(WidgetStyle::NO_BACKGROUND) {
            self.draw_background(&mut graphics, matrix);
        }
        if !self.base.is_destroyed() {
            self.draw_items(&mut graphics, matrix);
        }
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        if !self.base.is_enabled() || self.base.is_destroyed() {
            return;
        }
        match message.data.clone() {
            MessageData::SetFocus => self.on_focus(true),
            MessageData::KillFocus { .. } => self.on_focus(false),
            MessageData::Mouse {
                command: MouseCommand::LeftButtonDown,
                pos,
                flags,
            } => self.on_left_button_down(pos, flags),
            MessageData::Mouse {
                command: MouseCommand::LeftButtonUp,
                ..
            } => {
                if self.left_button_down.replace(false) {
                    self.base.set_grab(false);
                }
            }
            MessageData::MouseWheel { .. } => {
                if self.is_show_vert_scroll_bar() {
                    if let Some(bar) = self.get_vert_scroll_bar() {
                        dispatch_message_to(&(bar as WidgetRef), message);
                    }
                }
            }
            MessageData::Key {
                command: KeyCommand::KeyDown,
                key_code_or_char,
                flags,
            } if matches!(
                key_code_or_char,
                VKEY_TAB | VKEY_UP | VKEY_DOWN | VKEY_HOME | VKEY_END
            ) =>
            {
                let focused = self.get_focused_item();
                let item = self.get_list_item(focused.as_ref(), key_code_or_char);
                self.on_vk(
                    item.as_ref(),
                    flags & KEY_FLAG_SHIFT != 0,
                    flags & KEY_FLAG_CTRL != 0,
                );
            }
            _ => {}
        }
        if !self.base.is_destroyed() && message.destination.is_some() {
            self.base.on_process_message(message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        let EventData::Scroll { code, pos } = event.data else {
            return;
        };
        let Some(source) = event.source.as_ref() else {
            return;
        };
        for bar in [self.get_vert_scroll_bar(), self.get_horz_scroll_bar()]
            .into_iter()
            .flatten()
        {
            if source.base().id() == bar.base().id() {
                self.on_scroll(&bar, code, pos);
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "cfwl_listbox_tests.rs"]
mod tests;
