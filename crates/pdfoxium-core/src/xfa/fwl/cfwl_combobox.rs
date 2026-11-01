// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cfwl_combobox.{h,cpp}`. Child controls retain their own registered identity;
//! back-links resolve through the weak app/tree, avoiding owner cycles.
use super::{
    cfwl_app::CfwlApp,
    cfwl_comboedit::CfwlComboEdit,
    cfwl_combolist::CfwlComboList,
    cfwl_edit::{
        FWL_STYLEEXT_EDT_HALIGN_MASK, FWL_STYLEEXT_EDT_HALIGN_MODE_MASK, FWL_STYLEEXT_EDT_HCENTER,
        FWL_STYLEEXT_EDT_HNEAR, FWL_STYLEEXT_EDT_JUSTIFIED, FWL_STYLEEXT_EDT_READ_ONLY,
        FWL_STYLEEXT_EDT_VALIGN_MASK, FWL_STYLEEXT_EDT_VCENTER, FWL_STYLEEXT_EDT_VFAR,
        FWL_STYLEEXT_EDT_VNEAR,
    },
    cfwl_event::{EventData, EventType, FwlEvent},
    cfwl_listbox::{
        FWL_STYLEEXT_LTB_ALIGN_MASK, FWL_STYLEEXT_LTB_CENTER_ALIGN, FWL_STYLEEXT_LTB_LEFT_ALIGN,
    },
    cfwl_message::{FwlMessage, KeyCommand, MessageData, MouseCommand},
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{CfwlThemePart, Part, PartState},
    cfwl_widget::{
        dispatch_message_to, CfwlWidget, FwlType, FwlWidget, Properties, WidgetId, WidgetRef,
        WidgetState, WidgetStyle,
    },
    fwl_widgetdef::{VKEY_DOWN, VKEY_ESCAPE, VKEY_RETURN, VKEY_UP},
    fwl_widgethit::FwlWidgetHit,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{
    cell::{Cell, OnceCell},
    rc::Rc,
};

pub const STYLEEXT_CMB_DROP_DOWN: u32 = 1;
pub const STYLEEXT_CMB_SORT: u32 = 1 << 1;
pub const STYLEEXT_CMB_OWNER_DRAW: u32 = 1 << 3;
pub const STYLEEXT_CMB_EDIT_HNEAR: u32 = 0;
pub const STYLEEXT_CMB_EDIT_HCENTER: u32 = 1 << 4;
pub const STYLEEXT_CMB_EDIT_VNEAR: u32 = 0;
pub const STYLEEXT_CMB_EDIT_VCENTER: u32 = 1 << 6;
pub const STYLEEXT_CMB_EDIT_VFAR: u32 = 2 << 6;
pub const STYLEEXT_CMB_EDIT_JUSTIFIED: u32 = 1 << 8;
pub const STYLEEXT_CMB_EDIT_HALIGN_MASK: u32 = 3 << 4;
pub const STYLEEXT_CMB_EDIT_VALIGN_MASK: u32 = 3 << 6;
pub const STYLEEXT_CMB_LIST_ITEM_LEFT_ALIGN: u32 = 0;
pub const STYLEEXT_CMB_LIST_ITEM_CENTER_ALIGN: u32 = 1 << 10;
pub const STYLEEXT_CMB_LIST_ITEM_ALIGN_MASK: u32 = 3 << 10;
pub const STYLEEXT_CMB_READ_ONLY: u32 = 1 << 13;

pub struct CfwlComboBox {
    base: CfwlWidget,
    client_rect: Cell<CfxRectF>,
    content_rect: Cell<CfxRectF>,
    button_rect: Cell<CfxRectF>,
    edit: OnceCell<Rc<CfwlComboEdit>>,
    list_box: OnceCell<Rc<CfwlComboList>>,
    current_selection: Cell<i32>,
    button_state: Cell<PartState>,
    dropdown_generation: Cell<u64>,
}
impl CfwlComboBox {
    pub fn new(app: &Rc<CfwlApp>) -> Rc<Self> {
        Self::with_properties(app, Properties::default(), None)
    }
    pub fn with_properties(
        app: &Rc<CfwlApp>,
        properties: Properties,
        outer: Option<WidgetRef>,
    ) -> Rc<Self> {
        let control = Rc::new(Self {
            base: CfwlWidget::new(app, properties, outer),
            client_rect: Cell::new(CfxRectF::default()),
            content_rect: Cell::new(CfxRectF::default()),
            button_rect: Cell::new(CfxRectF::default()),
            edit: OnceCell::new(),
            list_box: OnceCell::new(),
            current_selection: Cell::new(-1),
            button_state: Cell::new(PartState::NORMAL),
            dropdown_generation: Cell::new(0),
        });
        // Registration itself invokes no host callbacks. Construct both child
        // identities before exposing any update, drawing or script operation.
        app.register_widget(control.clone());
        let parent: WidgetRef = control.clone();
        assert!(control
            .edit
            .set(CfwlComboEdit::new(
                app,
                Properties::default(),
                Some(parent.clone())
            ))
            .is_ok());
        assert!(control
            .list_box
            .set(CfwlComboList::new(
                app,
                Properties {
                    styles: WidgetStyle::BORDER | WidgetStyle::VSCROLL,
                    style_exts: 0,
                    states: WidgetState::INVISIBLE
                },
                Some(parent)
            ))
            .is_ok());
        control
    }
    fn edit(&self) -> &Rc<CfwlComboEdit> {
        self.edit.get().expect("constructed combo edit")
    }
    fn list(&self) -> &Rc<CfwlComboList> {
        self.list_box.get().expect("constructed combo list")
    }
    pub fn get_combo_edit(&self) -> Rc<CfwlComboEdit> {
        self.edit().clone()
    }
    pub fn get_combo_list(&self) -> Rc<CfwlComboList> {
        self.list().clone()
    }
    pub fn client_rect(&self) -> CfxRectF {
        self.client_rect.get()
    }
    pub fn button_rect(&self) -> CfxRectF {
        self.button_rect.get()
    }
    pub fn is_drop_down_style(&self) -> bool {
        self.base.get_style_exts() & STYLEEXT_CMB_DROP_DOWN != 0
    }
    pub fn is_drop_list_visible(&self) -> bool {
        self.list().base().is_visible()
    }
    pub fn get_cur_sel(&self) -> i32 {
        self.current_selection.get()
    }
    pub fn get_current_selection(&self) -> i32 {
        self.current_selection.get()
    }
    pub fn add_string(&self, text: &[u32]) {
        self.list().add_string(text);
    }
    pub fn remove_at(&self, index: i32) {
        if self.list().get_item(index).is_none() {
            return;
        }
        self.list().remove_at(index);
        let selected = self.current_selection.get();
        if selected == index {
            self.set_cur_sel(-1);
        } else if selected > index {
            self.current_selection.set(selected - 1);
        }
        if self.list().count_items() == 0 {
            self.hide_drop_down_list();
        }
    }
    pub fn remove_all(&self) {
        self.list().delete_all();
        if self.current_selection.get() >= 0 {
            self.set_cur_sel(-1);
        }
        self.hide_drop_down_list();
    }
    pub fn get_text_by_index(&self, index: i32) -> Vec<u32> {
        self.list()
            .get_item(index)
            .map_or_else(Vec::new, |item| item.text().to_vec())
    }
    pub fn set_cur_sel(&self, index: i32) {
        let index = if index >= 0 && index < self.list().count_items() {
            index
        } else {
            -1
        };
        self.current_selection.set(index);
        if self.is_drop_down_style() {
            self.edit().set_text(&self.get_text_by_index(index));
            if !self.base.is_destroyed() {
                self.edit().update();
            }
        }
    }
    pub fn set_edit_text(&self, text: &[u32]) {
        self.edit().set_text(text);
        if !self.base.is_destroyed() {
            self.edit().update();
        }
    }
    pub fn get_edit_text(&self) -> Vec<u32> {
        self.edit().get_text()
    }
    pub fn edit_can_undo(&self) -> bool {
        self.edit().can_undo()
    }
    pub fn edit_can_redo(&self) -> bool {
        self.edit().can_redo()
    }
    pub fn edit_undo(&self) -> bool {
        self.edit().undo()
    }
    pub fn edit_redo(&self) -> bool {
        self.edit().redo()
    }
    pub fn edit_can_copy(&self) -> bool {
        self.edit().has_selection()
    }
    pub fn edit_can_cut(&self) -> bool {
        self.edit().base().get_style_exts() & FWL_STYLEEXT_EDT_READ_ONLY == 0
            && self.edit_can_copy()
    }
    pub fn edit_can_select_all(&self) -> bool {
        self.edit().get_text_length() > 0
    }
    pub fn edit_copy(&self) -> Option<Vec<u32>> {
        self.edit().copy()
    }
    pub fn edit_cut(&self) -> Option<Vec<u32>> {
        self.edit().cut()
    }
    pub fn edit_paste(&self, text: &[u32]) -> bool {
        self.edit().paste(text)
    }
    pub fn edit_select_all(&self) {
        self.edit().select_all();
    }
    pub fn edit_delete(&self) {
        self.edit().clear_text();
    }
    pub fn edit_de_select(&self) {
        self.edit().clear_selection();
    }
    pub fn edit_modify_style_exts(&self, added: u32, removed: u32) {
        self.edit().modify_style_exts(added, removed);
    }
    pub fn get_bbox(&self) -> CfxRectF {
        let mut rect = self.base.get_widget_rect();
        if self.is_drop_list_visible() {
            let mut list = self.list().get_widget_rect();
            list.offset(rect.left, rect.top);
            rect.union(list);
        }
        rect
    }
    fn next_dropdown_generation(&self) -> u64 {
        let next = self.dropdown_generation.get().wrapping_add(1);
        self.dropdown_generation.set(next);
        next
    }
    fn still_opening(&self, generation: u64) -> bool {
        self.dropdown_generation.get() == generation && !self.base.is_destroyed()
    }
    pub fn show_drop_down_list(&self) {
        if self.is_drop_list_visible() || self.base.is_destroyed() {
            return;
        }
        let generation = self.next_dropdown_generation();
        let mut event = FwlEvent::new(self.base.self_ref(), EventData::PreDropDown);
        self.base.dispatch_event(&mut event);
        if event.source.is_none() || !self.still_opening(generation) {
            return;
        }
        let count = self.list().count_items();
        if count < 1 {
            return;
        }
        self.reset_list_item_alignment();
        self.list().change_selected(self.current_selection.get());
        if !self.still_opening(generation) {
            return;
        }
        let item_height = self.list().calc_item_height();
        let border = self.base.get_cx_border_size();
        let min = if count > 3 {
            item_height * 3. + border * 2.
        } else {
            0.
        };
        let max = item_height * count as f32 + border * 2.;
        let widget_rect = self.base.get_widget_rect();
        let mut rect = CfxRectF::new(self.client_rect.get().left, 0., widget_rect.width, 0.);
        let (Some(app), Some(owner)) = (self.base.app(), self.base.self_ref()) else {
            return;
        };
        if !app
            .widget_mgr()
            .get_adapter_popup_pos(&owner, min, max, widget_rect, &mut rect)
            || !self.still_opening(generation)
        {
            return;
        }
        if ![rect.left, rect.top, rect.width, rect.height]
            .into_iter()
            .all(f32::is_finite)
            || rect.width <= 0.
            || rect.height <= 0.
        {
            return;
        }
        self.list().base().set_widget_rect(rect);
        self.list().update();
        if !self.still_opening(generation) {
            return;
        }
        self.list().reset_notify_owner();
        self.list().clear_states(WidgetState::INVISIBLE);
        let mut post = FwlEvent::new(self.base.self_ref(), EventData::PostDropDown);
        self.base.dispatch_event(&mut post);
        if !self.base.is_destroyed() {
            self.repaint_inflated_list_box_rect();
        }
    }
    pub fn hide_drop_down_list(&self) {
        // A hide requested by a PreDropDown callback cancels that pending show.
        self.next_dropdown_generation();
        if !self.is_drop_list_visible() {
            return;
        }
        self.list().set_states(WidgetState::INVISIBLE);
        self.repaint_inflated_list_box_rect();
    }
    fn repaint_inflated_list_box_rect(&self) {
        let mut rect = self.list().get_widget_rect();
        rect.inflate_xy(2., 2.);
        self.base.repaint_rect(rect);
    }
    fn match_edit_text(&self) {
        let text = self.edit().get_text();
        let index = self.list().match_item(&text);
        if index != self.current_selection.get() {
            self.list().change_selected(index);
            if index >= 0 {
                self.sync_edit_text(index);
            }
        } else if index >= 0 {
            self.edit().set_selected();
        }
        if !self.base.is_destroyed() {
            self.current_selection.set(index);
        }
    }
    fn sync_edit_text(&self, index: i32) {
        self.edit().set_text(&self.get_text_by_index(index));
        if self.base.is_destroyed() {
            return;
        }
        self.edit().update();
        self.edit().set_selected();
    }
    fn layout(&self) {
        let client = self.get_client_rect();
        self.client_rect.set(client);
        let Some(app) = self.base.app() else {
            return;
        };
        let theme = app.theme_provider();
        let button_width = theme.get_scroll_bar_width();
        self.button_rect.set(
            if self.base.get_style_exts() & STYLEEXT_CMB_READ_ONLY == 0 {
                CfxRectF::new(
                    client.right() - button_width,
                    client.top + 1.,
                    button_width - 1.,
                    client.height - 2.,
                )
            } else {
                CfxRectF::default()
            },
        );
        let mut content = client;
        let margin = theme.get_ui_margin(&CfwlThemePart::new(Part::None, self));
        content.deflate(margin.left, margin.top, margin.width, margin.height);
        self.content_rect.set(content);
        if !self.is_drop_down_style() || self.base.is_destroyed() {
            return;
        }
        self.edit().base().set_widget_rect(CfxRectF::new(
            content.left,
            content.top,
            (content.width - button_width).max(0.),
            content.height,
        ));
        if self.current_selection.get() >= 0 {
            let _lock = self.edit().base().scoped_update_lock();
            self.edit()
                .set_text(&self.get_text_by_index(self.current_selection.get()));
        }
        if !self.base.is_destroyed() {
            self.edit().update();
        }
    }
    fn reset_edit_alignment(&self) {
        let styles = self.base.get_style_exts();
        let mut add = if styles & STYLEEXT_CMB_EDIT_HALIGN_MASK == STYLEEXT_CMB_EDIT_HCENTER {
            FWL_STYLEEXT_EDT_HCENTER
        } else {
            FWL_STYLEEXT_EDT_HNEAR
        };
        add |= match styles & STYLEEXT_CMB_EDIT_VALIGN_MASK {
            STYLEEXT_CMB_EDIT_VCENTER => FWL_STYLEEXT_EDT_VCENTER,
            STYLEEXT_CMB_EDIT_VFAR => FWL_STYLEEXT_EDT_VFAR,
            _ => FWL_STYLEEXT_EDT_VNEAR,
        };
        if styles & STYLEEXT_CMB_EDIT_JUSTIFIED != 0 {
            add |= FWL_STYLEEXT_EDT_JUSTIFIED;
        }
        self.edit().modify_style_exts(
            add,
            FWL_STYLEEXT_EDT_HALIGN_MASK
                | FWL_STYLEEXT_EDT_HALIGN_MODE_MASK
                | FWL_STYLEEXT_EDT_VALIGN_MASK,
        );
    }
    fn reset_list_item_alignment(&self) {
        let added = if self.base.get_style_exts() & STYLEEXT_CMB_LIST_ITEM_ALIGN_MASK
            == STYLEEXT_CMB_LIST_ITEM_CENTER_ALIGN
        {
            FWL_STYLEEXT_LTB_CENTER_ALIGN
        } else {
            FWL_STYLEEXT_LTB_LEFT_ALIGN
        };
        self.list()
            .modify_style_exts(added, FWL_STYLEEXT_LTB_ALIGN_MASK);
    }
    pub fn process_sel_changed(&self, left_button_up: bool) {
        self.current_selection.set(
            self.list()
                .get_item_index(self.list().get_sel_item(0).as_ref()),
        );
        if !self.is_drop_down_style() {
            self.base.repaint_rect(self.client_rect.get());
            return;
        }
        let Some(item) = self.list().get_item(self.current_selection.get()) else {
            return;
        };
        self.edit().set_text(item.text());
        if self.base.is_destroyed() {
            return;
        }
        self.edit().update();
        self.edit().set_selected();
        let mut event = FwlEvent::select_changed(self.base.self_ref(), left_button_up);
        self.base.dispatch_event(&mut event);
    }
    fn on_focus(&self, gained: bool) {
        if gained {
            self.base.set_states(WidgetState::FOCUSED);
            if !self
                .edit()
                .base()
                .get_states()
                .contains(WidgetState::FOCUSED)
            {
                let edit: WidgetRef = self.edit().clone();
                let mut msg = FwlMessage::set_focus(Some(edit.clone()));
                dispatch_message_to(&edit, &mut msg);
            }
        } else {
            self.base.clear_states(WidgetState::FOCUSED);
            self.hide_drop_down_list();
            let edit: WidgetRef = self.edit().clone();
            let mut msg = FwlMessage::kill_focus(None);
            dispatch_message_to(&edit, &mut msg);
        }
    }
    fn on_key(&self, message: &mut FwlMessage) {
        let MessageData::Key {
            command,
            key_code_or_char,
            ..
        } = message.data
        else {
            return;
        };
        if command == KeyCommand::KeyDown && matches!(key_code_or_char, VKEY_UP | VKEY_DOWN) {
            let count = self.list().count_items();
            if count < 1 {
                return;
            }
            let text = self.edit().get_text();
            let mut index = self.list().match_item(&text);
            let equal = self
                .list()
                .get_item(index)
                .is_some_and(|item| item.text() == text);
            if index < 0 {
                index = 0;
            } else if equal {
                if (key_code_or_char == VKEY_UP && index == 0)
                    || (key_code_or_char == VKEY_DOWN && index == count - 1)
                {
                    return;
                }
                index += if key_code_or_char == VKEY_UP { -1 } else { 1 };
            }
            self.current_selection.set(index);
            self.sync_edit_text(index);
            return;
        }
        let edit: WidgetRef = self.edit().clone();
        dispatch_message_to(&edit, message);
    }
}
impl FwlWidget for CfwlComboBox {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::ComboBox
    }
    fn as_combo_box(&self) -> Option<&CfwlComboBox> {
        Some(self)
    }
    fn draws_child(&self, id: WidgetId) -> bool {
        self.edit.get().is_some_and(|edit| edit.base().id() == id)
            || self
                .list_box
                .get()
                .is_some_and(|list| list.base().id() == id)
    }
    fn update(&self) {
        if !self.base.is_locked() && !self.base.is_destroyed() {
            self.reset_edit_alignment();
            self.layout();
        }
    }
    fn modify_style_exts(&self, added: u32, mut removed: u32) {
        let add_dropdown = added & STYLEEXT_CMB_DROP_DOWN != 0;
        let remove_dropdown = removed & STYLEEXT_CMB_DROP_DOWN != 0;
        removed &= !STYLEEXT_CMB_DROP_DOWN;
        self.base.modify_style_exts(STYLEEXT_CMB_DROP_DOWN, 0);
        if add_dropdown {
            self.edit().modify_style_exts(0, FWL_STYLEEXT_EDT_READ_ONLY);
        } else if remove_dropdown {
            self.edit().modify_style_exts(FWL_STYLEEXT_EDT_READ_ONLY, 0);
        }
        self.base.modify_style_exts(added, removed);
    }
    fn set_states(&self, states: WidgetState) {
        if self.is_drop_down_style() {
            self.edit().set_states(states);
        }
        self.list().set_states(states);
        self.base.set_states(states);
    }
    fn clear_states(&self, states: WidgetState) {
        if self.is_drop_down_style() {
            self.edit().clear_states(states);
        }
        self.list().clear_states(states);
        self.base.clear_states(states);
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        let rect = self.base.get_widget_rect();
        let text = CfxRectF::new(
            0.,
            0.,
            rect.width - self.button_rect.get().width,
            rect.height,
        );
        if text.contains_point(point) {
            FwlWidgetHit::Edit
        } else if self.button_rect.get().contains_point(point)
            || (self.is_drop_list_visible() && self.list().get_widget_rect().contains_point(point))
        {
            FwlWidgetHit::Client
        } else {
            FwlWidgetHit::Unknown
        }
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics, matrix: CfxMatrix) {
        if !self.button_rect.get().is_empty_with_epsilon(0.1) {
            if let Some(app) = self.base.app() {
                let mut state = graphics.state_restorer();
                state.concat_matrix(matrix);
                let mut params = CfwlThemeBackground::new(Part::DropDownButton, self);
                params.states = self.button_state.get();
                params.part_rect = self.button_rect.get();
                app.theme_provider().draw_background(&params, &mut state);
            }
        }
        let rect = self.edit().get_widget_rect();
        let mut m = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
        m.concat(matrix);
        self.edit().draw_widget(graphics, m);
        if self.is_drop_list_visible() {
            let rect = self.list().get_widget_rect();
            let mut m = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
            m.concat(matrix);
            self.list().draw_widget(graphics, m);
        }
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        if self.base.is_destroyed() || !self.base.is_enabled() {
            return;
        }
        match &message.data {
            MessageData::SetFocus => self.on_focus(true),
            MessageData::KillFocus { .. } => self.on_focus(false),
            MessageData::Mouse { command, pos, .. } => match command {
                MouseCommand::LeftButtonDown => {
                    if self.is_drop_list_visible() {
                        if self.button_rect.get().contains_point(*pos) {
                            self.hide_drop_down_list();
                        }
                    } else if self.client_rect.get().contains_point(*pos) {
                        self.match_edit_text();
                        self.show_drop_down_list();
                    }
                }
                MouseCommand::LeftButtonUp => {
                    self.button_state
                        .set(if self.button_rect.get().contains_point(*pos) {
                            PartState::HOVERED
                        } else {
                            PartState::NORMAL
                        });
                    self.base.repaint_rect(self.button_rect.get());
                }
                _ => {}
            },
            MessageData::Key {
                command,
                key_code_or_char,
                ..
            } => {
                if self.is_drop_list_visible()
                    && *command == KeyCommand::KeyDown
                    && matches!(
                        *key_code_or_char,
                        VKEY_UP | VKEY_DOWN | VKEY_RETURN | VKEY_ESCAPE
                    )
                {
                    let list: WidgetRef = self.list().clone();
                    dispatch_message_to(&list, message);
                } else {
                    self.on_key(message);
                }
            }
            _ => self.base.on_process_message(message),
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        if self.base.is_destroyed() {
            return;
        }
        match event.get_type() {
            EventType::Scroll => {
                if let EventData::Scroll { code, pos } = event.data {
                    let mut forwarded = FwlEvent::scroll(self.base.self_ref(), code, pos);
                    self.base.dispatch_event(&mut forwarded);
                }
            }
            EventType::TextWillChange => {
                let mut changed = FwlEvent::new(self.base.self_ref(), EventData::EditChanged);
                self.base.dispatch_event(&mut changed);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "cfwl_combobox_tests.rs"]
mod tests;
