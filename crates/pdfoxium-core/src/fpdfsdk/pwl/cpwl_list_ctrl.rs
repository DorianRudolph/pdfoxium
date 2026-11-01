// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `fpdfsdk/pwl/cpwl_list_ctrl.{h,cpp}`. The list owns item editors; notifier
//! and host links remain weak. Individual storage borrows end before callbacks,
//! allowing a window callback to resize, change selection or detach its list.

use super::cpwl_edit_impl::{CpwlEditImpl, OwnedEditFontMapProvider};
use crate::fpdfdoc::cpvt_variabletext::Provider;
use crate::fpdfdoc::ipvt_fontmap::IpvtFontMap;
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

fn equal(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.0001
}
fn bigger(a: f32, b: f32) -> bool {
    a > b && !equal(a, b)
}
fn smaller(a: f32, b: f32) -> bool {
    a < b && !equal(a, b)
}
fn uppercase(word: u32) -> u32 {
    let Some(c) = char::from_u32(word) else {
        return word;
    };
    let mut chars = c.to_uppercase();
    let result = chars.next().unwrap();
    // towupper is a single-codepoint mapping, not Unicode case expansion.
    if chars.next().is_some() {
        word
    } else {
        u32::from(result)
    }
}

pub trait ListNotify {
    fn set_scroll_info_y(
        &self,
        plate_min: f32,
        plate_max: f32,
        content_min: f32,
        content_max: f32,
        small_step: f32,
        big_step: f32,
    );
    fn set_scroll_pos_y(&self, position: f32);
    /// False means the receiving window was destroyed by the notification.
    fn invalidate_rect(&self, rect: CfxFloatRect) -> bool;
}

struct Item {
    selected: Cell<bool>,
    rect: Cell<CfxFloatRect>,
    edit: Rc<RefCell<CpwlEditImpl<'static>>>,
}
impl Item {
    fn new(
        font_map: Option<Rc<dyn IpvtFontMap>>,
        font_size: f32,
        direction: ParagraphDirection,
        text: &[u32],
    ) -> Self {
        let provider = font_map
            .map(|font_map| Rc::new(OwnedEditFontMapProvider { font_map }) as Rc<dyn Provider>);
        let mut edit = CpwlEditImpl::new_owned(provider);
        edit.set_alignment_v(1);
        edit.initialize();
        edit.set_font_size(font_size);
        edit.paint();
        edit.set_text_direction(direction);
        edit.set_text(text);
        edit.paint();
        Self {
            selected: Cell::new(false),
            rect: Cell::new(CfxFloatRect::default()),
            edit: Rc::new(RefCell::new(edit)),
        }
    }
    fn height(&self) -> f32 {
        self.edit.borrow().content_rect().height()
    }
    fn text(&self) -> Vec<u32> {
        self.edit.borrow().text()
    }
    fn first_char(&self) -> u16 {
        let edit = self.edit.borrow();
        let mut iterator = edit.iterator();
        iterator.set_at_index(1);
        iterator.word().map_or(0, |word| word.word)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Selection {
    Deselecting,
    Normal,
    Selecting,
}
#[derive(Default)]
struct SelectState {
    items: BTreeMap<i32, Selection>,
}
impl SelectState {
    fn add(&mut self, index: i32) {
        self.items.insert(index, Selection::Selecting);
    }
    fn add_range(&mut self, begin: i32, end: i32) {
        for index in begin.min(end)..=begin.max(end) {
            self.add(index);
        }
    }
    fn sub(&mut self, index: i32) {
        if let Some(state) = self.items.get_mut(&index) {
            *state = Selection::Deselecting;
        }
    }
    fn sub_range(&mut self, begin: i32, end: i32) {
        // Source visits every index; the ordered map yields the same entries
        // directly, without work proportional to a potentially sparse range.
        for (_, state) in self.items.range_mut(begin.min(end)..=begin.max(end)) {
            *state = Selection::Deselecting;
        }
    }
    fn deselect_all(&mut self) {
        for state in self.items.values_mut() {
            *state = Selection::Deselecting;
        }
    }
    fn done(&mut self) {
        self.items.retain(|_, state| {
            if *state == Selection::Deselecting {
                false
            } else {
                *state = Selection::Normal;
                true
            }
        });
    }
}

pub struct CpwlListCtrl {
    plate_rect: Cell<CfxFloatRect>,
    content_rect: Cell<CfxFloatRect>,
    scroll_pos: Cell<CfxPointF>,
    font_size: Cell<f32>,
    direction: Cell<ParagraphDirection>,
    selected_index: Cell<i32>,
    selection: RefCell<SelectState>,
    foot_index: Cell<i32>,
    caret_index: Cell<i32>,
    ctrl_select: Cell<bool>,
    multiple: Cell<bool>,
    notifying: Cell<bool>,
    notify: RefCell<Option<Weak<dyn ListNotify>>>,
    items: RefCell<Vec<Rc<Item>>>,
    font_map: RefCell<Option<Rc<dyn IpvtFontMap>>>,
}
impl Default for CpwlListCtrl {
    fn default() -> Self {
        Self {
            plate_rect: Cell::new(CfxFloatRect::default()),
            content_rect: Cell::new(CfxFloatRect::default()),
            scroll_pos: Cell::new(CfxPointF::default()),
            font_size: Cell::new(0.),
            direction: Cell::new(ParagraphDirection::Auto),
            selected_index: Cell::new(-1),
            selection: RefCell::new(SelectState::default()),
            foot_index: Cell::new(-1),
            caret_index: Cell::new(-1),
            ctrl_select: Cell::new(false),
            multiple: Cell::new(false),
            notifying: Cell::new(false),
            notify: RefCell::new(None),
            items: RefCell::new(Vec::new()),
            font_map: RefCell::new(None),
        }
    }
}
impl Drop for CpwlListCtrl {
    fn drop(&mut self) {
        self.items.get_mut().clear();
        self.invalidate_item(-1);
    }
}
impl CpwlListCtrl {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_notify(&self, notify: Option<&Rc<dyn ListNotify>>) {
        *self.notify.borrow_mut() = notify.map(Rc::downgrade);
    }
    pub fn set_font_map(&self, font_map: Option<Rc<dyn IpvtFontMap>>) {
        *self.font_map.borrow_mut() = font_map;
    }
    pub fn set_font_size(&self, size: f32) {
        self.font_size.set(size);
    }
    pub fn font_size(&self) -> f32 {
        self.font_size.get()
    }
    pub fn set_text_direction(&self, direction: ParagraphDirection) {
        self.direction.set(direction);
    }
    pub fn set_multiple_sel(&self, multiple: bool) {
        self.multiple.set(multiple);
    }
    pub fn is_multiple_sel(&self) -> bool {
        self.multiple.get()
    }
    pub fn plate_rect(&self) -> CfxFloatRect {
        self.plate_rect.get()
    }
    pub fn caret(&self) -> i32 {
        self.caret_index.get()
    }
    pub fn selected_index(&self) -> i32 {
        self.selected_index.get()
    }
    pub fn count(&self) -> i32 {
        i32::try_from(self.items.borrow().len()).unwrap_or(i32::MAX)
    }
    fn item(&self, index: i32) -> Option<Rc<Item>> {
        self.items
            .borrow()
            .get(usize::try_from(index).ok()?)
            .cloned()
    }
    fn valid(&self, index: i32) -> bool {
        index >= 0 && index < self.count()
    }
    pub fn item_edit(&self, index: i32) -> Option<Rc<RefCell<CpwlEditImpl<'static>>>> {
        self.item(index).map(|item| Rc::clone(&item.edit))
    }
    fn in_to_out(&self, point: CfxPointF) -> CfxPointF {
        let scroll = self.scroll_pos.get();
        let plate = self.plate_rect.get();
        CfxPointF::new(
            point.x - (scroll.x - plate.left),
            point.y - (scroll.y - plate.top),
        )
    }
    fn out_to_in(&self, point: CfxPointF) -> CfxPointF {
        let scroll = self.scroll_pos.get();
        let plate = self.plate_rect.get();
        CfxPointF::new(
            point.x + (scroll.x - plate.left),
            point.y + (scroll.y - plate.top),
        )
    }
    fn in_to_out_rect(&self, rect: CfxFloatRect) -> CfxFloatRect {
        let lb = self.in_to_out(CfxPointF::new(rect.left, rect.bottom));
        let rt = self.in_to_out(CfxPointF::new(rect.right, rect.top));
        CfxFloatRect::new(lb.x, lb.y, rt.x, rt.y)
    }
    fn inner_to_outer(&self, point: CfxPointF) -> CfxPointF {
        let plate = self.plate_rect.get();
        CfxPointF::new(point.x + plate.left, plate.top - point.y)
    }
    fn outer_to_inner(&self, point: CfxPointF) -> CfxPointF {
        let plate = self.plate_rect.get();
        CfxPointF::new(point.x - plate.left, plate.top - point.y)
    }
    fn inner_to_outer_rect(&self, rect: CfxFloatRect) -> CfxFloatRect {
        let lt = self.inner_to_outer(CfxPointF::new(rect.left, rect.top));
        let rb = self.inner_to_outer(CfxPointF::new(rect.right, rect.bottom));
        CfxFloatRect::new(lt.x, rb.y, rb.x, lt.y)
    }
    pub fn on_mouse_down(&self, point: CfxPointF, shift: bool, ctrl: bool) {
        let index = self.item_index(point);
        if self.is_multiple_sel() {
            if ctrl {
                if self.is_item_selected(index) {
                    self.selection.borrow_mut().sub(index);
                    self.select_items();
                    self.ctrl_select.set(false);
                } else {
                    self.selection.borrow_mut().add(index);
                    self.select_items();
                    self.ctrl_select.set(true);
                }
                self.foot_index.set(index);
            } else if shift {
                {
                    let mut selection = self.selection.borrow_mut();
                    selection.deselect_all();
                    selection.add_range(self.foot_index.get(), index);
                }
                self.select_items();
            } else {
                {
                    let mut selection = self.selection.borrow_mut();
                    selection.deselect_all();
                    selection.add(index);
                }
                self.select_items();
                self.foot_index.set(index);
            }
            self.set_caret(index);
        } else {
            self.set_single_select(index);
        }
        if !self.is_item_visible(index) {
            self.scroll_to_list_item(index);
        }
    }
    pub fn on_mouse_move(&self, point: CfxPointF, _shift: bool, ctrl: bool) {
        let index = self.item_index(point);
        if self.is_multiple_sel() {
            {
                let mut selection = self.selection.borrow_mut();
                if ctrl {
                    if self.ctrl_select.get() {
                        selection.add_range(self.foot_index.get(), index);
                    } else {
                        selection.sub_range(self.foot_index.get(), index);
                    }
                } else {
                    selection.deselect_all();
                    selection.add_range(self.foot_index.get(), index);
                }
            }
            self.select_items();
            self.set_caret(index);
        } else {
            self.set_single_select(index);
        }
        if !self.is_item_visible(index) {
            self.scroll_to_list_item(index);
        }
    }
    fn on_vk(&self, index: i32, shift: bool, ctrl: bool) {
        if self.is_multiple_sel() {
            if self.valid(index) {
                if !ctrl {
                    {
                        let mut selection = self.selection.borrow_mut();
                        selection.deselect_all();
                        if shift {
                            selection.add_range(self.foot_index.get(), index);
                        } else {
                            selection.add(index);
                        }
                    }
                    self.select_items();
                    if !shift {
                        self.foot_index.set(index);
                    }
                }
                self.set_caret(index);
            }
        } else {
            self.set_single_select(index);
        }
        if !self.is_item_visible(index) {
            self.scroll_to_list_item(index);
        }
    }
    pub fn on_vk_up(&self, shift: bool, ctrl: bool) {
        self.on_vk(
            if self.is_multiple_sel() {
                self.caret()
            } else {
                self.selected_index()
            }
            .saturating_sub(1),
            shift,
            ctrl,
        );
    }
    pub fn on_vk_down(&self, shift: bool, ctrl: bool) {
        self.on_vk(
            if self.is_multiple_sel() {
                self.caret()
            } else {
                self.selected_index()
            }
            .saturating_add(1),
            shift,
            ctrl,
        );
    }
    pub fn on_vk_left(&self, shift: bool, ctrl: bool) {
        self.on_vk(0, shift, ctrl);
    }
    pub fn on_vk_right(&self, shift: bool, ctrl: bool) {
        self.on_vk(self.count() - 1, shift, ctrl);
    }
    pub fn on_vk_home(&self, shift: bool, ctrl: bool) {
        self.on_vk(0, shift, ctrl);
    }
    pub fn on_vk_end(&self, shift: bool, ctrl: bool) {
        self.on_vk(self.count() - 1, shift, ctrl);
    }
    pub fn on_char(&self, character: u16, shift: bool, ctrl: bool) -> bool {
        let index = self.last_selected();
        let found = self.find_next(index, u32::from(character));
        if found == index {
            return false;
        }
        self.on_vk(found, shift, ctrl);
        true
    }
    pub fn set_plate_rect(&self, rect: CfxFloatRect) {
        self.plate_rect.set(rect);
        let mut scroll = self.scroll_pos.get();
        scroll.x = rect.left;
        self.scroll_pos.set(scroll);
        self.set_scroll_pos(CfxPointF::new(rect.left, rect.top));
        self.rearrange(0);
        self.invalidate_item(-1);
    }
    pub fn item_rect(&self, index: i32) -> CfxFloatRect {
        self.in_to_out_rect(self.item_rect_internal(index))
    }
    fn item_rect_internal(&self, index: i32) -> CfxFloatRect {
        let Some(item) = self.item(index) else {
            return CfxFloatRect::default();
        };
        let mut rect = item.rect.get();
        rect.left = 0.;
        rect.right = self.plate_rect.get().width();
        self.inner_to_outer_rect(rect)
    }
    pub fn add_string(&self, text: &[u32]) {
        let font_map = self.font_map.borrow().clone();
        let item = Rc::new(Item::new(
            font_map,
            self.font_size.get(),
            self.direction.get(),
            text,
        ));
        self.items.borrow_mut().push(item);
        self.rearrange(self.count() - 1);
    }
    fn set_multiple_select(&self, index: i32, selected: bool) {
        let Some(item) = self.item(index) else {
            return;
        };
        if item.selected.replace(selected) != selected {
            self.invalidate_item(index);
        }
    }
    fn set_single_select(&self, index: i32) {
        if !self.valid(index) || self.selected_index.get() == index {
            return;
        }
        let old = self.selected_index.get();
        if old >= 0 {
            self.set_item_select(old, false);
            self.invalidate_item(old);
        }
        self.set_item_select(index, true);
        self.invalidate_item(index);
        self.selected_index.set(index);
    }
    pub fn set_caret(&self, index: i32) {
        if !self.valid(index) || !self.is_multiple_sel() {
            return;
        }
        let old = self.caret_index.replace(index);
        if old != index {
            self.invalidate_item(old);
            self.invalidate_item(index);
        }
    }
    fn notifier(&self) -> Option<Rc<dyn ListNotify>> {
        if self.notifying.get() {
            return None;
        }
        self.notify.borrow().as_ref().and_then(Weak::upgrade)
    }
    fn invalidate_item(&self, index: i32) {
        let Some(notify) = self.notifier() else {
            return;
        };
        let mut rect = if index == -1 {
            self.plate_rect.get()
        } else {
            self.item_rect(index)
        };
        if index != -1 {
            rect.left -= 1.;
            rect.right += 1.;
            rect.bottom -= 1.;
            rect.top += 1.;
        }
        self.notifying.set(true);
        if !notify.invalidate_rect(rect) {
            *self.notify.borrow_mut() = None;
        }
        self.notifying.set(false);
    }
    fn select_items(&self) {
        // Snapshot just indices/state: no selection-map borrow crosses a host
        // callback, and item text/layout storage remains in place.
        let changes: Vec<_> = self
            .selection
            .borrow()
            .items
            .iter()
            .filter(|(_, state)| **state != Selection::Normal)
            .map(|(&index, &state)| (index, state))
            .collect();
        for (index, state) in changes {
            self.set_multiple_select(index, state == Selection::Selecting);
        }
        self.selection.borrow_mut().done();
    }
    pub fn select(&self, index: i32) {
        if !self.valid(index) {
            return;
        }
        if self.is_multiple_sel() {
            self.selection.borrow_mut().add(index);
            self.select_items();
        } else {
            self.set_single_select(index);
        }
    }
    pub fn deselect(&self, index: i32) {
        if !self.is_item_selected(index) {
            return;
        }
        self.set_multiple_select(index, false);
        if !self.is_multiple_sel() {
            self.selected_index.set(-1);
        }
    }
    pub fn is_item_visible(&self, index: i32) -> bool {
        let plate = self.plate_rect.get();
        let item = self.item_rect(index);
        item.bottom >= plate.bottom && item.top <= plate.top
    }
    pub fn scroll_to_list_item(&self, index: i32) {
        if !self.valid(index) {
            return;
        }
        let plate = self.plate_rect.get();
        let item = self.item_rect_internal(index);
        let item_ctrl = self.item_rect(index);
        if smaller(item_ctrl.bottom, plate.bottom) {
            if smaller(item_ctrl.top, plate.top) {
                self.set_scroll_pos_y(item.bottom + plate.height());
            }
        } else if bigger(item_ctrl.top, plate.top) && bigger(item_ctrl.bottom, plate.bottom) {
            self.set_scroll_pos_y(item.top);
        }
    }
    fn set_scroll_info(&self) {
        let Some(notify) = self.notifier() else {
            return;
        };
        let plate = self.plate_rect.get();
        let content = self.content_rect_internal();
        self.notifying.set(true);
        notify.set_scroll_info_y(
            plate.bottom,
            plate.top,
            content.bottom,
            content.top,
            self.first_height(),
            plate.height(),
        );
        self.notifying.set(false);
    }
    pub fn set_scroll_pos(&self, point: CfxPointF) {
        self.set_scroll_pos_y(point.y);
    }
    pub fn scroll_pos(&self) -> CfxPointF {
        self.scroll_pos.get()
    }
    fn set_scroll_pos_y(&self, mut y: f32) {
        if equal(self.scroll_pos.get().y, y) {
            return;
        }
        let plate = self.plate_rect.get();
        let content = self.content_rect_internal();
        if plate.height() > content.height() {
            y = plate.top;
        } else if smaller(y - plate.height(), content.bottom) {
            y = content.bottom + plate.height();
        } else if bigger(y, content.top) {
            y = content.top;
        }
        let mut scroll = self.scroll_pos.get();
        scroll.y = y;
        self.scroll_pos.set(scroll);
        self.invalidate_item(-1);
        if let Some(notify) = self.notifier() {
            self.notifying.set(true);
            notify.set_scroll_pos_y(y);
            self.notifying.set(false);
        }
    }
    fn content_rect_internal(&self) -> CfxFloatRect {
        self.inner_to_outer_rect(self.content_rect.get())
    }
    pub fn content_rect(&self) -> CfxFloatRect {
        self.in_to_out_rect(self.content_rect_internal())
    }
    fn rearrange(&self, index: i32) {
        if self.items.borrow().is_empty() {
            self.content_rect.set(CfxFloatRect::default());
            self.set_scroll_info();
            return;
        }
        let index = index.max(0).min(self.count() - 1);
        let mut position = self
            .item(index - 1)
            .map_or(0., |item| item.rect.get().bottom);
        {
            let items = self.items.borrow();
            for item in &items[index as usize..] {
                let height = item.height();
                item.rect
                    .set(CfxFloatRect::new(0., position + height, 0., position));
                position += height;
            }
        }
        self.content_rect
            .set(CfxFloatRect::new(0., position, 0., 0.));
        self.set_scroll_info();
    }
    pub fn set_top_item(&self, index: i32) {
        if self.valid(index) {
            self.set_scroll_pos_y(self.item_rect_internal(index).top);
        }
    }
    pub fn top_item(&self) -> i32 {
        let plate = self.plate_rect.get();
        let mut index = self.item_index(CfxPointF::new(plate.left, plate.top));
        if !self.is_item_visible(index) && self.is_item_visible(index.saturating_add(1)) {
            index = index.saturating_add(1);
        }
        index
    }
    pub fn item_index(&self, point: CfxPointF) -> i32 {
        let point = self.outer_to_inner(self.out_to_in(point));
        let (mut first, mut last) = (true, true);
        for (index, item) in self.items.borrow().iter().enumerate() {
            let rect = item.rect.get();
            if bigger(point.y, rect.top) {
                first = false;
            }
            if smaller(point.y, rect.bottom) {
                last = false;
            }
            if point.y >= rect.top && point.y < rect.bottom {
                return index as i32;
            }
        }
        if first {
            0
        } else if last {
            self.count() - 1
        } else {
            -1
        }
    }
    pub fn text(&self) -> Vec<u32> {
        self.item_text(if self.is_multiple_sel() {
            self.caret_index.get()
        } else {
            self.selected_index.get()
        })
    }
    pub fn item_text(&self, index: i32) -> Vec<u32> {
        self.item(index).map_or_else(Vec::new, |item| item.text())
    }
    pub fn first_height(&self) -> f32 {
        self.item(0).map_or(1., |item| item.height())
    }
    pub fn first_selected(&self) -> i32 {
        self.items
            .borrow()
            .iter()
            .position(|item| item.selected.get())
            .map_or(-1, |index| index as i32)
    }
    fn last_selected(&self) -> i32 {
        self.items
            .borrow()
            .iter()
            .rposition(|item| item.selected.get())
            .map_or(-1, |index| index as i32)
    }
    pub fn find_next(&self, index: i32, character: u32) -> i32 {
        let count = self.count();
        let mut circle_index = index;
        for _ in 0..count {
            circle_index = circle_index.saturating_add(1);
            if circle_index >= count || circle_index < 0 {
                circle_index = 0;
            }
            if self
                .item(circle_index)
                .is_some_and(|item| uppercase(u32::from(item.first_char())) == uppercase(character))
            {
                return circle_index;
            }
        }
        // With no prior selection, the source's final loop index accidentally
        // selects the last option on a failed search. A failed type search
        // leaves the original selection unchanged instead.
        index
    }
    pub fn is_item_selected(&self, index: i32) -> bool {
        self.item(index).is_some_and(|item| item.selected.get())
    }
    fn set_item_select(&self, index: i32, selected: bool) {
        if let Some(item) = self.item(index) {
            item.selected.set(selected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::font::cpdf_font::CpdfFont;
    use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
    use crate::fxcrt::fx_codepage::FxCharset;
    use crate::fxge::cfx_gemodule::with_ge_module;

    struct FontMap {
        font: CpdfFont,
    }
    impl IpvtFontMap for FontMap {
        fn get_pdf_font(&self, index: i32) -> Option<CpdfFont> {
            (index == 0).then(|| self.font.clone())
        }
        fn get_pdf_font_alias(&self, index: i32) -> Vec<u8> {
            if index == 0 {
                b"Helv".to_vec()
            } else {
                Vec::new()
            }
        }
        fn get_word_font_index(&self, _: u16, _: FxCharset, _: i32) -> i32 {
            0
        }
        fn char_code_from_unicode(&self, index: i32, word: u16) -> i32 {
            if index == 0 {
                self.font.char_code_from_unicode(u32::from(word)) as i32
            } else {
                -1
            }
        }
        fn charset_from_unicode(&self, _: u16, charset: FxCharset) -> FxCharset {
            charset
        }
    }
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn list() -> Rc<CpwlListCtrl> {
        let data = Rc::new(CpdfDocPageData::new());
        let font = with_ge_module(|module| {
            data.get_stock_font(b"Helvetica", &mut module.font_mgr().builtin_mapper_mut())
        })
        .unwrap();
        let list = Rc::new(CpwlListCtrl::new());
        list.set_font_map(Some(Rc::new(FontMap {
            font: CpdfFont::Simple(font),
        })));
        list.set_font_size(10.);
        for text in ["Alpha", "Beta", "Bravo", "Delta", "Éclair"] {
            list.add_string(&wide(text));
        }
        let h = list.first_height();
        assert!(h > 0.);
        list.set_plate_rect(CfxFloatRect::new(10., 20., 110., 20. + h * 2.));
        list
    }
    fn point(list: &CpwlListCtrl, index: i32) -> CfxPointF {
        let rect = list.item_rect(index);
        CfxPointF::new(
            (rect.left + rect.right) * 0.5,
            (rect.bottom + rect.top) * 0.5,
        )
    }
    fn selected(list: &CpwlListCtrl) -> Vec<i32> {
        (0..list.count())
            .filter(|&i| list.is_item_selected(i))
            .collect()
    }

    #[test]
    fn single_selection_navigation_casefold_search_and_invalid_indices() {
        let list = list();
        assert_eq!(list.selected_index(), -1);
        assert!(!list.on_char(b'Z' as u16, false, false));
        assert_eq!(list.selected_index(), -1);
        list.on_vk_down(false, false);
        assert_eq!(list.text(), wide("Alpha"));
        assert!(list.on_char(b'b' as u16, false, false));
        assert_eq!(list.selected_index(), 1);
        assert!(list.on_char(b'B' as u16, false, false));
        assert_eq!(list.selected_index(), 2);
        assert!(list.on_char(b'b' as u16, false, false));
        assert_eq!(list.selected_index(), 1);
        assert_eq!(list.find_next(3, 0x00e9), 4);
        list.on_vk_end(false, false);
        assert_eq!(list.selected_index(), 4);
        list.on_vk_down(false, false);
        assert_eq!(list.selected_index(), 4);
        list.on_vk_home(false, false);
        assert_eq!(list.selected_index(), 0);
        list.on_vk_up(false, false);
        assert_eq!(list.selected_index(), 0);
        list.select(-1);
        list.select(i32::MAX);
        list.deselect(3);
        assert_eq!(selected(&list), [0]);
        list.deselect(0);
        assert_eq!(list.selected_index(), -1);
        assert!(list.text().is_empty());
    }

    #[test]
    fn multiple_mouse_ranges_ctrl_toggle_and_keyboard_caret() {
        let list = list();
        list.set_multiple_sel(true);
        list.on_mouse_down(point(&list, 1), false, false);
        assert_eq!(selected(&list), [1]);
        assert_eq!(list.caret(), 1);
        list.on_mouse_down(point(&list, 3), true, false);
        assert_eq!(selected(&list), [1, 2, 3]);
        assert_eq!(list.caret(), 3);
        list.on_mouse_down(point(&list, 2), false, true);
        assert_eq!(selected(&list), [1, 3]);
        list.on_mouse_move(point(&list, 3), false, true);
        assert_eq!(selected(&list), [1]);
        list.on_vk_home(false, true);
        assert_eq!(list.caret(), 0);
        assert_eq!(selected(&list), [1]);
        list.on_vk_end(false, false);
        assert_eq!(selected(&list), [4]);
        assert_eq!(list.text(), wide("Éclair"));
        list.on_vk_up(true, false);
        assert_eq!(selected(&list), [3, 4]);
        list.deselect(4);
        assert_eq!(selected(&list), [3]);
        assert_eq!(list.first_selected(), 3);
    }

    #[test]
    fn item_layout_scroll_clamps_and_top_item_tracks_plate() {
        let list = list();
        let h = list.first_height();
        // Leave a small margin above the two fully visible rows. Exact row/
        // viewport alignment can differ by a few ulps after scroll transforms,
        // while the source's visibility check uses strict inequalities.
        let mut plate = list.plate_rect();
        plate.top += 0.5;
        list.set_plate_rect(plate);
        assert_eq!(list.top_item(), 0);
        assert_eq!(list.item_index(CfxPointF::new(15., plate.top + 100.)), 0);
        assert_eq!(list.item_index(CfxPointF::new(15., plate.bottom - 100.)), 4);
        for index in 0..5 {
            let rect = list.item_rect(index);
            assert!((rect.height() - h).abs() < 0.001);
            assert_eq!(rect.left, 10.);
            assert_eq!(rect.right, 110.);
            assert_eq!(list.item_index(point(&list, index)), index);
        }
        assert!(list.item_edit(-1).is_none());
        assert!(list.item_edit(5).is_none());
        list.scroll_to_list_item(4);
        assert!(list.is_item_visible(4));
        assert_eq!(list.top_item(), 3);
        list.set_scroll_pos(CfxPointF::new(999., -10000.));
        assert_eq!(list.top_item(), 3);
        list.set_scroll_pos(CfxPointF::new(-999., 10000.));
        assert_eq!(list.top_item(), 0);
        assert_eq!(list.scroll_pos().x, 10.);
        list.set_top_item(1);
        assert_eq!(list.top_item(), 1);
        let rect = list.item_rect(1);
        assert!((rect.top - plate.top).abs() < 0.001);
        assert!(list.content_rect().height() >= h * 5. - 0.001);
    }

    struct Notify {
        list: Weak<CpwlListCtrl>,
        calls: Cell<usize>,
        retain: Cell<bool>,
    }
    impl ListNotify for Notify {
        fn set_scroll_info_y(&self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
            let list = self.list.upgrade().unwrap();
            list.set_scroll_pos(CfxPointF::new(0., list.plate_rect().top));
        }
        fn set_scroll_pos_y(&self, _: f32) {}
        fn invalidate_rect(&self, _: CfxFloatRect) -> bool {
            self.calls.set(self.calls.get() + 1);
            let list = self.list.upgrade().unwrap();
            // Resize and query during invalidation without retaining a borrow
            // of the item vector, editor or selection map in the dispatcher.
            list.set_plate_rect(list.plate_rect());
            assert_eq!(list.count(), 5);
            self.retain.get()
        }
    }
    #[test]
    fn weak_notification_reentry_and_destroyed_receiver_cancels_callbacks() {
        let list = list();
        let notify = Rc::new(Notify {
            list: Rc::downgrade(&list),
            calls: Cell::new(0),
            retain: Cell::new(false),
        });
        let iface: Rc<dyn ListNotify> = notify.clone();
        list.set_notify(Some(&iface));
        list.select(0);
        assert_eq!(notify.calls.get(), 1);
        list.select(1);
        assert_eq!(notify.calls.get(), 1);
        notify.retain.set(true);
        list.set_notify(Some(&iface));
        list.select(2);
        assert!(notify.calls.get() > 1);
        drop(iface);
        drop(notify);
        list.select(3);
        assert_eq!(list.selected_index(), 3);
    }
}
