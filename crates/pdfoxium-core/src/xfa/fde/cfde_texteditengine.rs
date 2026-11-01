// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `CFDE_TextEditEngine` port. The source gap buffer, bounded operation
//! ring and FGAS layout remain recognizable. Operations retain text, never a
//! pointer back to their owner. Delegates receive an explicit mutable engine;
//! no interior-mutability borrow is held while a callback executes.

use super::{
    cfde_data::FdeTextStyle,
    cfde_textout::CfdeTextOut,
    cfde_wordbreak_data::{
        check_state_change_for_word_break, get_word_break_property, WordBreakProperty,
    },
};
use crate::{
    fxcrt::fx_coordinates::{CfxPointF, CfxRectF},
    fxge::text_char_pos::TextCharPos,
    xfa::fgas::{
        font::cfgas_gefont::CfgasGeFont,
        layout::{
            cfgas_break::LayoutStyle,
            cfgas_char::BreakType,
            cfgas_txtbreak::{
                break_type_none_or_piece, CfgasTxtBreak, TxtBreakEngine, TxtBreakRun,
                ALIGNMENT_CENTER, ALIGNMENT_LEFT, ALIGNMENT_RIGHT, FX_TXTCHARSTYLE_ODD_BIDI_LEVEL,
            },
        },
    },
};
use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FdeTextEditPiece {
    pub rect: CfxRectF,
    pub start: i32,
    pub count: i32,
    pub bidi_level: i32,
    pub char_styles: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RecordOperation {
    #[default]
    InsertRecord,
    SkipRecord,
    SkipNotify,
}

#[derive(Clone, Debug, Default)]
pub struct TextChange {
    pub text: Vec<u32>,
    pub previous_text: Vec<u32>,
    pub selection_start: usize,
    pub selection_end: usize,
    pub cancelled: bool,
}

/// Shared callback owner, with mutable state ordinarily kept in small Cells.
/// Passing the engine explicitly permits safe callback reentry without an
/// `Rc<RefCell<Engine>>` borrow spanning script or host execution.
pub trait TextEditDelegate {
    fn notify_text_full(&self, _engine: &mut CfdeTextEditEngine) {}
    fn on_caret_changed(&self, _engine: &mut CfdeTextEditEngine) {}
    fn on_text_will_change(&self, _engine: &mut CfdeTextEditEngine, _change: &mut TextChange) {}
    fn on_text_changed(&self, _engine: &mut CfdeTextEditEngine) {}
    fn on_sel_changed(&self, _engine: &mut CfdeTextEditEngine) {}
    fn on_validate(&self, _engine: &mut CfdeTextEditEngine, _text: &[u32]) -> bool {
        true
    }
    fn set_scroll_offset(&self, _engine: &mut CfdeTextEditEngine, _offset: f32) {}
}

#[derive(Clone)]
enum Operation {
    Insert {
        start: usize,
        text: Vec<u32>,
    },
    Delete {
        start: usize,
        text: Vec<u32>,
    },
    Replace {
        start: usize,
        removed: Vec<u32>,
        added: Vec<u32>,
    },
}
impl Operation {
    fn edit(&self, redo: bool) -> (usize, usize, &[u32]) {
        match (self, redo) {
            (Self::Insert { start, text }, true) | (Self::Delete { start, text }, false) => {
                (*start, 0, text)
            }
            (Self::Insert { start, text }, false) | (Self::Delete { start, text }, true) => {
                (*start, text.len(), &[])
            }
            (
                Self::Replace {
                    start,
                    removed,
                    added,
                },
                true,
            ) => (*start, removed.len(), added),
            (
                Self::Replace {
                    start,
                    removed,
                    added,
                },
                false,
            ) => (*start, added.len(), removed),
        }
    }
}

/// A validated edit. Reserving the gap precedes any removal; host validation
/// may re-enter the engine, so ranges are clamped again before this is built.
struct PreparedEdit {
    start: usize,
    remove: usize,
    added: Vec<u32>,
    exceeded_limit: bool,
}

const GAP_SIZE: usize = 128;
const MAX_EDIT_OPERATIONS: usize = 128;
const PAGE_WIDTH_MAX: usize = 0xffff;

pub struct CfdeTextEditEngine {
    contents_bounding_box: CfxRectF,
    delegate: Option<Rc<dyn TextEditDelegate>>,
    text_piece_info: Vec<FdeTextEditPiece>,
    char_widths: Vec<i32>,
    text_break: CfgasTxtBreak,
    font: Option<Rc<CfgasGeFont>>,
    font_color: u32,
    last_insert_full: bool,
    font_size: f32,
    line_spacing: f32,
    content: Vec<u32>,
    text_length: usize,
    gap_position: usize,
    gap_size: usize,
    available_width: usize,
    character_limit: usize,
    visible_line_count: usize,
    operation_buffer: Vec<Option<Rc<Operation>>>,
    next_operation_index_to_undo: usize,
    next_operation_index_to_insert: usize,
    max_edit_operations: usize,
    character_alignment: i32,
    has_character_limit: bool,
    is_comb_text: bool,
    is_dirty: bool,
    validation_enabled: bool,
    is_multiline: bool,
    is_linewrap_enabled: bool,
    limit_horizontal_area: bool,
    limit_vertical_area: bool,
    password_mode: bool,
    password_alias: u32,
    has_selection: bool,
    selection_start: usize,
    selection_count: usize,
}
impl Default for CfdeTextEditEngine {
    fn default() -> Self {
        Self::new()
    }
}
impl CfdeTextEditEngine {
    pub fn new() -> Self {
        let mut text_break = CfgasTxtBreak::new();
        text_break.set_font_size(10.);
        text_break.set_line_break_tolerance(2.);
        text_break.set_tab_width(36.);
        // Keep the breaker consistent with the declared single-line/no-wrap
        // defaults. The source leaves its 100-point multiline defaults live,
        // then the idempotent setters skip applying the requested false mode.
        text_break.set_layout_styles(LayoutStyle::SINGLE_LINE);
        text_break.set_line_width(PAGE_WIDTH_MAX as f32);
        Self {
            contents_bounding_box: CfxRectF::default(),
            delegate: None,
            text_piece_info: Vec::new(),
            char_widths: Vec::new(),
            text_break,
            font: None,
            font_color: 0xff000000,
            last_insert_full: false,
            font_size: 10.,
            line_spacing: 10.,
            content: vec![0; GAP_SIZE],
            text_length: 0,
            gap_position: 0,
            gap_size: GAP_SIZE,
            available_width: PAGE_WIDTH_MAX,
            character_limit: usize::MAX,
            visible_line_count: 1,
            operation_buffer: vec![None; MAX_EDIT_OPERATIONS],
            next_operation_index_to_undo: MAX_EDIT_OPERATIONS - 1,
            next_operation_index_to_insert: 0,
            max_edit_operations: MAX_EDIT_OPERATIONS,
            character_alignment: ALIGNMENT_LEFT,
            has_character_limit: false,
            is_comb_text: false,
            is_dirty: false,
            validation_enabled: false,
            is_multiline: false,
            is_linewrap_enabled: false,
            limit_horizontal_area: false,
            limit_vertical_area: false,
            password_mode: false,
            password_alias: u32::from(b'*'),
            has_selection: false,
            selection_start: 0,
            selection_count: 0,
        }
    }
    pub fn set_delegate(&mut self, delegate: Option<Rc<dyn TextEditDelegate>>) {
        self.delegate = delegate;
    }
    pub fn clear(&mut self) {
        self.text_length = 0;
        self.gap_position = 0;
        self.gap_size = GAP_SIZE;
        self.content.clear();
        self.content.resize(GAP_SIZE, 0);
        self.clear_selection();
        self.clear_operation_records();
        // The source leaves stale geometry after clearing the text.
        self.is_dirty = true;
    }
    pub fn set_max_edit_operations_for_testing(&mut self, max: usize) {
        self.max_edit_operations = max;
        self.operation_buffer.resize(max, None);
        self.clear_operation_records();
    }
    fn adjust_gap(&mut self, index: usize, length: usize) -> bool {
        let index = index.min(self.text_length);
        if index < self.gap_position {
            self.content
                .copy_within(index..self.gap_position, index + self.gap_size);
            self.gap_position = index;
        } else if index > self.gap_position {
            self.content.copy_within(
                self.gap_position + self.gap_size..index + self.gap_size,
                self.gap_position,
            );
            self.gap_position = index;
        }
        if length >= self.gap_size {
            let Some(new_gap_size) = length.checked_add(GAP_SIZE) else {
                return false;
            };
            let Some(new_size) = self.text_length.checked_add(new_gap_size) else {
                return false;
            };
            if self
                .content
                .try_reserve(new_size.saturating_sub(self.content.len()))
                .is_err()
            {
                return false;
            }
            self.content.resize(new_size, 0);
            self.content.copy_within(
                self.gap_position + self.gap_size..self.text_length + self.gap_size,
                self.gap_position + new_gap_size,
            );
            self.gap_size = new_gap_size;
        }
        true
    }
    fn count_chars_exceeding_size(&self, text: &[u32], num_to_check: usize) -> usize {
        if !self.limit_horizontal_area && !self.limit_vertical_area {
            return 0;
        }
        let Some(font) = self.font.clone() else {
            return 0;
        };
        let mut text_out = CfdeTextOut::new();
        text_out.set_line_space(self.line_spacing);
        text_out.set_font(font);
        text_out.set_font_size(self.font_size);
        text_out.set_styles(FdeTextStyle {
            single_line: !self.is_multiline,
            line_wrap: self.is_linewrap_enabled,
            last_line_height: false,
        });
        let width = if self.is_linewrap_enabled {
            self.available_width
        } else {
            PAGE_WIDTH_MAX
        } as f32;
        let height = self.line_spacing * self.visible_line_count as f32;
        for exceeding in 0..num_to_check.min(text.len()) {
            let mut rect = CfxRectF::new(0., 0., width, 0.);
            text_out.calc_logic_rect(&text[..text.len() - exceeding], &mut rect);
            // Both enabled constraints must hold; the source's separate early
            // breaks could accept text that satisfied only one dimension.
            if (!self.limit_horizontal_area || rect.width <= self.available_width as f32)
                && (!self.limit_vertical_area || rect.height <= height)
            {
                return exceeding;
            }
        }
        num_to_check.min(text.len())
    }
    /// Outcome consumed by an FWL adapter after releasing its engine borrow.
    pub fn last_insert_was_full(&self) -> bool {
        self.last_insert_full
    }
    /// Validation candidate for a history insertion, without executing host code.
    pub fn history_validation_text(&self, redo: bool) -> Option<Vec<u32>> {
        let index = if redo {
            if !self.can_redo() {
                return None;
            }
            (self.next_operation_index_to_undo + 1) % self.max_edit_operations
        } else {
            if !self.can_undo() {
                return None;
            }
            self.next_operation_index_to_undo
        };
        let operation = self.operation_buffer.get(index)?.as_ref()?;
        let (start, removed, added): (usize, usize, &[u32]) = match (operation.as_ref(), redo) {
            (Operation::Insert { start, text }, true)
            | (Operation::Delete { start, text }, false) => (*start, 0, text),
            (
                Operation::Replace {
                    start,
                    removed,
                    added,
                },
                true,
            ) => (*start, removed.len(), added),
            (
                Operation::Replace {
                    start,
                    removed,
                    added,
                },
                false,
            ) => (*start, added.len(), removed),
            _ => return None,
        };
        let mut text = self.get_text();
        let start = start.min(text.len());
        let end = start.saturating_add(removed).min(text.len());
        text.splice(start..end, added.iter().copied());
        Some(text)
    }
    pub fn insert(&mut self, index: usize, request_text: &[u32], record: RecordOperation) {
        self.last_insert_full = false;
        if request_text.is_empty() {
            return;
        }
        let mut index = index.min(self.text_length);
        let mut remove = 0;
        let mut text = request_text.to_vec();
        if record == RecordOperation::InsertRecord {
            if let Some(delegate) = self.delegate.clone() {
                let mut change = TextChange {
                    selection_start: index,
                    selection_end: index,
                    text,
                    previous_text: self.get_text(),
                    cancelled: false,
                };
                delegate.on_text_will_change(self, &mut change);
                if change.cancelled {
                    return;
                }
                text = change.text;
                index = change.selection_start.min(self.text_length);
                remove = change.selection_end.saturating_sub(change.selection_start);
            }
        }
        if let Some(edit) = self.prepare_edit(index, remove, text, record) {
            self.commit_edit(edit, record, false, false);
        }
    }
    fn prepare_edit(
        &mut self,
        mut index: usize,
        mut remove: usize,
        mut text: Vec<u32>,
        record: RecordOperation,
    ) -> Option<PreparedEdit> {
        index = index.min(self.text_length);
        remove = remove.min(self.text_length - index);
        let mut length = text.len();
        let mut exceeded_limit = false;
        // Empty replacements are deletions, whose source path does not invoke
        // insertion validation. Ordinary inserts keep their existing fast path.
        if length != 0 {
            self.last_insert_full = false;
            if self.has_character_limit
                && length
                    > self
                        .character_limit
                        .saturating_sub(self.text_length - remove)
            {
                if record == RecordOperation::SkipNotify {
                    self.character_limit = (self.text_length - remove).saturating_add(length);
                } else {
                    length = self
                        .character_limit
                        .saturating_sub(self.text_length - remove);
                    exceeded_limit = true;
                }
            }
            if self.validation_enabled || self.limit_horizontal_area || self.limit_vertical_area {
                let mut candidate = self.get_text();
                candidate.splice(index..index + remove, text[..length].iter().copied());
                if self.validation_enabled {
                    if let Some(delegate) = self.delegate.clone() {
                        if !delegate.on_validate(self, &candidate) {
                            return None;
                        }
                        // Preserve intentional host edits; only recompute the
                        // pending edit against the callback's current buffer.
                        index = index.min(self.text_length);
                        remove = remove.min(self.text_length - index);
                        if self.has_character_limit && record != RecordOperation::SkipNotify {
                            let permitted = self
                                .character_limit
                                .saturating_sub(self.text_length - remove);
                            exceeded_limit |= length > permitted;
                            length = length.min(permitted);
                        }
                        candidate = self.get_text();
                        candidate.splice(index..index + remove, text[..length].iter().copied());
                    }
                }
                let exceeding = self.count_chars_exceeding_size(&candidate, length);
                if exceeding > 0 {
                    length -= exceeding;
                    exceeded_limit = true;
                }
            }
            self.last_insert_full = exceeded_limit;
        }
        text.truncate(length);
        // Removing the old range makes that storage available for insertion.
        // Reserve before deleting, so allocation failure cannot erase content.
        if (remove != 0 || length != 0)
            && !self.adjust_gap(index + remove, length.saturating_sub(remove))
        {
            return None;
        }
        Some(PreparedEdit {
            start: index,
            remove,
            added: text,
            exceeded_limit,
        })
    }
    fn commit_edit(
        &mut self,
        edit: PreparedEdit,
        record: RecordOperation,
        replace: bool,
        return_removed: bool,
    ) -> Vec<u32> {
        let PreparedEdit {
            start,
            remove,
            added,
            exceeded_limit,
        } = edit;
        if remove == 0 && added.is_empty() {
            if exceeded_limit {
                if let Some(delegate) = self.delegate.clone() {
                    delegate.notify_text_full(self);
                }
            }
            return Vec::new();
        }
        let mut removed = if record == RecordOperation::InsertRecord || return_removed {
            self.content[start..start + remove].to_vec()
        } else {
            Vec::new()
        };
        self.gap_position = start;
        self.gap_size += remove;
        self.text_length -= remove;
        self.content[start..start + added.len()].copy_from_slice(&added);
        self.gap_position += added.len();
        self.gap_size -= added.len();
        self.text_length += added.len();
        self.is_dirty = true;
        self.clear_selection();
        if record == RecordOperation::InsertRecord {
            // Only Delete's public return value and its history record need
            // separate ownership. Other replacements move the one range copy.
            let saved_removed = if return_removed {
                removed.clone()
            } else {
                std::mem::take(&mut removed)
            };
            let operation = if replace || (remove != 0 && !added.is_empty()) {
                Operation::Replace {
                    start,
                    removed: saved_removed,
                    added,
                }
            } else if added.is_empty() {
                Operation::Delete {
                    start,
                    text: saved_removed,
                }
            } else {
                Operation::Insert { start, text: added }
            };
            self.add_operation_record(operation);
        }
        if let Some(delegate) = self.delegate.clone() {
            if exceeded_limit {
                delegate.notify_text_full(self);
            }
            delegate.on_text_changed(self);
        }
        removed
    }
    fn add_operation_record(&mut self, operation: Operation) {
        if self.max_edit_operations == 0 {
            return;
        }
        let mut last = if self.next_operation_index_to_insert == 0 {
            self.max_edit_operations - 1
        } else {
            self.next_operation_index_to_insert - 1
        };
        if self.next_operation_index_to_undo != last {
            if self.next_operation_index_to_undo > last {
                while last != 0 {
                    self.operation_buffer[last] = None;
                    last -= 1;
                }
                self.operation_buffer[0] = None;
                last = self.max_edit_operations - 1;
            }
            while self.next_operation_index_to_undo != last {
                self.operation_buffer[last] = None;
                last -= 1;
            }
        }
        last = (last + 1) % self.max_edit_operations;
        self.operation_buffer[last] = Some(Rc::new(operation));
        self.next_operation_index_to_insert = (last + 1) % self.max_edit_operations;
        self.next_operation_index_to_undo = last;
    }
    pub fn clear_operation_records(&mut self) {
        self.operation_buffer.fill(None);
        self.next_operation_index_to_undo = self.max_edit_operations.saturating_sub(1);
        self.next_operation_index_to_insert = 0;
    }
    pub fn can_undo(&self) -> bool {
        self.operation_buffer
            .get(self.next_operation_index_to_undo)
            .is_some_and(Option::is_some)
            && self.next_operation_index_to_undo != self.next_operation_index_to_insert
    }
    pub fn can_redo(&self) -> bool {
        if self.max_edit_operations == 0 {
            return false;
        }
        let index = (self.next_operation_index_to_undo + 1) % self.max_edit_operations;
        index != self.next_operation_index_to_insert && self.operation_buffer[index].is_some()
    }
    pub fn undo(&mut self) -> bool {
        self.apply_history(false)
    }
    pub fn redo(&mut self) -> bool {
        self.apply_history(true)
    }
    fn apply_history(&mut self, redo: bool) -> bool {
        self.last_insert_full = false;
        if if redo {
            !self.can_redo()
        } else {
            !self.can_undo()
        } {
            return false;
        }
        let old_undo = self.next_operation_index_to_undo;
        let old_insert = self.next_operation_index_to_insert;
        let index = if redo {
            (old_undo + 1) % self.max_edit_operations
        } else {
            old_undo
        };
        let operation = self.operation_buffer[index].clone().unwrap();
        let (start, remove, added) = operation.edit(redo);
        let requested_length = added.len();
        let Some(edit) =
            self.prepare_edit(start, remove, added.to_vec(), RecordOperation::SkipRecord)
        else {
            return false;
        };
        // Validation may edit history itself. Never overwrite that decision or
        // consume a record after a rejected/truncated restoration.
        if edit.added.len() != requested_length
            || self.next_operation_index_to_undo != old_undo
            || self.next_operation_index_to_insert != old_insert
            || !self.operation_buffer[index]
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, &operation))
        {
            return false;
        }
        // Advance before change notifications so nested undo cannot repeat the
        // same operation or overwrite a cursor changed by a nested edit.
        self.next_operation_index_to_undo = if redo {
            index
        } else if old_undo == 0 {
            self.max_edit_operations - 1
        } else {
            old_undo - 1
        };
        self.commit_edit(edit, RecordOperation::SkipRecord, false, false);
        true
    }
    pub fn delete(&mut self, start: usize, length: usize, record: RecordOperation) -> Vec<u32> {
        if start >= self.text_length {
            return Vec::new();
        }
        let mut start = start;
        let mut length = length;
        let mut added = Vec::new();
        if record == RecordOperation::InsertRecord {
            if let Some(delegate) = self.delegate.clone() {
                let mut change = TextChange {
                    text: Vec::new(),
                    previous_text: self.get_text(),
                    selection_start: start,
                    selection_end: start.saturating_add(length),
                    cancelled: false,
                };
                delegate.on_text_will_change(self, &mut change);
                if change.cancelled {
                    return Vec::new();
                }
                start = change.selection_start;
                length = change.selection_end.saturating_sub(start);
                added = change.text;
                if start >= self.text_length {
                    return Vec::new();
                }
            }
        }
        self.prepare_edit(start, length, added, record)
            .map(|edit| self.commit_edit(edit, record, false, true))
            .unwrap_or_default()
    }
    pub fn replace_selected_text(&mut self, requested: &[u32]) {
        self.last_insert_full = false;
        let mut start = self.selection_start;
        let mut remove = self.selection_count;
        let mut text = requested.to_vec();
        if let Some(delegate) = self.delegate.clone() {
            let mut change = TextChange {
                selection_start: start,
                selection_end: start.saturating_add(remove),
                text,
                previous_text: self.get_text(),
                cancelled: false,
            };
            delegate.on_text_will_change(self, &mut change);
            if change.cancelled {
                return;
            }
            text = change.text;
            start = change.selection_start;
            remove = change.selection_end.saturating_sub(start);
        }
        if let Some(edit) = self.prepare_edit(start, remove, text, RecordOperation::InsertRecord) {
            self.commit_edit(edit, RecordOperation::InsertRecord, true, false);
        }
    }
    pub fn get_length(&self) -> usize {
        self.text_length
    }
    pub fn get_text(&self) -> Vec<u32> {
        let mut text = Vec::with_capacity(self.text_length);
        text.extend_from_slice(&self.content[..self.gap_position]);
        text.extend_from_slice(&self.content[self.gap_position + self.gap_size..]);
        text
    }
    pub fn get_char(&self, index: usize) -> u32 {
        if index >= self.text_length {
            return 0;
        }
        if self.password_mode {
            return self.password_alias;
        }
        self.content[if index < self.gap_position {
            index
        } else {
            index + self.gap_size
        }]
    }
    pub fn get_width_of_char(&mut self, index: usize) -> i32 {
        self.layout();
        self.char_widths.get(index).copied().unwrap_or(0)
    }
    pub fn select_all(&mut self) {
        if self.text_length == 0 {
            return;
        }
        self.has_selection = true;
        self.selection_start = 0;
        self.selection_count = self.text_length;
    }
    pub fn clear_selection(&mut self) {
        self.has_selection = false;
        self.selection_start = 0;
        self.selection_count = 0;
    }
    pub fn set_selection(&mut self, start: usize, count: usize) {
        if count == 0 {
            self.clear_selection();
            return;
        }
        if start > self.text_length {
            return;
        }
        self.has_selection = true;
        self.selection_start = start;
        self.selection_count = count.min(self.text_length - start);
    }
    pub fn has_selection(&self) -> bool {
        self.has_selection
    }
    pub fn get_selection(&self) -> (usize, usize) {
        (self.selection_start, self.selection_count)
    }
    pub fn get_selected_text(&self) -> Vec<u32> {
        if !self.has_selection {
            return Vec::new();
        }
        let start = self.selection_start.min(self.text_length);
        let end = start
            .saturating_add(self.selection_count)
            .min(self.text_length);
        let mut text = Vec::with_capacity(end - start);
        if start < self.gap_position {
            text.extend_from_slice(&self.content[start..end.min(self.gap_position)]);
        }
        if end > self.gap_position {
            text.extend_from_slice(
                &self.content[start.max(self.gap_position) + self.gap_size..end + self.gap_size],
            );
        }
        text
    }
    pub fn delete_selected_text(&mut self, record: RecordOperation) -> Vec<u32> {
        if !self.has_selection {
            return Vec::new();
        }
        self.delete(self.selection_start, self.selection_count, record)
    }
    pub fn get_index_left(&self, position: usize) -> usize {
        let mut position = position.min(self.text_length);
        if position == 0 {
            return 0;
        }
        position -= 1;
        // A caret denotes the boundary before `position`. Only the interior
        // of a CRLF pair must be skipped. Skipping preceding newlines also
        // skipped a visible character at the start of every line.
        if position != 0 && self.get_char(position) == 10 && self.get_char(position - 1) == 13 {
            position -= 1;
        }
        position
    }
    pub fn get_index_right(&self, position: usize) -> usize {
        if position >= self.text_length {
            return self.text_length;
        }
        let mut position = position + 1;
        if position < self.text_length
            && self.get_char(position) == 10
            && self.get_char(position - 1) == 13
        {
            position += 1;
        }
        position
    }
    pub fn get_index_at_start_of_line(&self, position: usize) -> usize {
        let mut position = position.min(self.text_length);
        if position > 0 && self.get_char(position) == 10 && self.get_char(position - 1) == 13 {
            position -= 1;
        }
        while position > 0 && !matches!(self.get_char(position - 1), 10 | 13) {
            position -= 1;
        }
        position
    }
    pub fn get_index_at_end_of_line(&self, position: usize) -> usize {
        let mut position = position.min(self.text_length);
        if position > 0 && self.get_char(position) == 10 && self.get_char(position - 1) == 13 {
            position -= 1;
        }
        while position < self.text_length && !matches!(self.get_char(position), 10 | 13) {
            position += 1;
        }
        position
    }
    pub fn get_index_up(&self, position: usize) -> usize {
        let position = position.min(self.text_length);
        let line_start = self.get_index_at_start_of_line(position);
        if line_start == 0 {
            return position;
        }
        let distance = position - line_start;
        // Cross exactly one line terminator, preserving empty lines. The
        // preceding line's end is the caret before its CR, LF or CRLF.
        let mut prior_end = line_start - 1;
        if prior_end > 0 && self.get_char(prior_end) == 10 && self.get_char(prior_end - 1) == 13 {
            prior_end -= 1;
        }
        let prior = self.get_index_at_start_of_line(prior_end);
        prior + distance.min(prior_end - prior)
    }
    pub fn get_index_down(&self, position: usize) -> usize {
        let position = position.min(self.text_length);
        let mut line_end = self.get_index_at_end_of_line(position);
        if line_end == self.text_length {
            return position;
        }
        let carriage_return = self.get_char(line_end) == 13;
        line_end += 1;
        if carriage_return && self.get_char(line_end) == 10 {
            line_end += 1;
        }
        if line_end == self.text_length {
            return line_end;
        }
        let distance = position - self.get_index_at_start_of_line(position);
        let next_end = self.get_index_at_end_of_line(line_end);
        line_end + distance.min(next_end - line_end)
    }
    pub fn limit_horizontal_scroll(&mut self, value: bool) {
        self.clear_operation_records();
        self.limit_horizontal_area = value;
    }
    pub fn limit_vertical_scroll(&mut self, value: bool) {
        self.clear_operation_records();
        self.limit_vertical_area = value;
    }
    pub fn set_available_width(&mut self, width: usize) {
        if width == self.available_width {
            return;
        }
        self.clear_operation_records();
        self.available_width = width;
        if self.is_linewrap_enabled {
            self.text_break.set_line_width(width as f32);
        }
        if self.is_comb_text {
            self.set_comb_text_width();
        }
        self.is_dirty = true;
    }
    pub fn set_has_character_limit(&mut self, value: bool) {
        if self.has_character_limit == value {
            return;
        }
        self.has_character_limit = value;
        self.character_limit = self.character_limit.max(self.text_length);
        if self.is_comb_text {
            self.set_comb_text_width();
        }
        self.is_dirty = true;
    }
    pub fn set_character_limit(&mut self, limit: usize) {
        if self.character_limit == limit {
            return;
        }
        self.clear_operation_records();
        self.character_limit = limit.max(self.text_length);
        if self.is_comb_text {
            self.set_comb_text_width();
        }
        self.is_dirty = true;
    }
    pub fn set_font(&mut self, font: Option<Rc<CfgasGeFont>>) {
        if match (&self.font, &font) {
            (None, None) => true,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        } {
            return;
        }
        self.font = font.clone();
        if let Some(font) = font {
            self.text_break.set_font(font);
        } else {
            self.text_break.set_break_status();
            self.text_break.font = None;
        }
        self.is_dirty = true;
    }
    pub fn get_font(&self) -> Option<Rc<CfgasGeFont>> {
        self.font.clone()
    }
    pub fn set_font_size(&mut self, size: f32) {
        if !size.is_finite() || size <= 0. || self.font_size == size {
            return;
        }
        self.font_size = size;
        self.text_break.set_font_size(size);
        self.is_dirty = true;
    }
    pub fn get_font_size(&self) -> f32 {
        self.font_size
    }
    pub fn set_font_color(&mut self, color: u32) {
        self.font_color = color;
    }
    pub fn get_font_color(&self) -> u32 {
        self.font_color
    }
    pub fn set_alignment(&mut self, alignment: i32) {
        if self.character_alignment == alignment {
            return;
        }
        self.character_alignment = alignment;
        self.text_break.set_alignment(alignment);
        self.is_dirty = true;
    }
    pub fn set_line_space(&mut self, space: f32) {
        if !space.is_finite() || space <= 0. || self.line_spacing == space {
            return;
        }
        self.line_spacing = space;
        self.is_dirty = true;
    }
    pub fn get_line_space(&self) -> f32 {
        self.line_spacing
    }
    pub fn set_alias_char(&mut self, alias: u32) {
        if self.password_alias != alias {
            self.password_alias = alias;
            self.is_dirty = true;
        }
    }
    pub fn set_tab_width(&mut self, width: f32) {
        if !width.is_finite() {
            return;
        }
        let old = self.text_break.get_tab_width();
        self.text_break.set_tab_width(width);
        self.is_dirty |= old != self.text_break.get_tab_width();
    }
    pub fn set_visible_line_count(&mut self, count: usize) {
        let count = count.max(1);
        if self.visible_line_count != count {
            self.visible_line_count = count;
            self.is_dirty = true;
        }
    }
    pub fn enable_validation(&mut self, value: bool) {
        self.validation_enabled = value;
    }
    pub fn enable_password_mode(&mut self, value: bool) {
        if self.password_mode != value {
            self.password_mode = value;
            self.is_dirty = true;
        }
    }
    pub fn enable_multi_line(&mut self, value: bool) {
        if self.is_multiline == value {
            return;
        }
        self.is_multiline = value;
        let mut styles = self.text_break.get_layout_styles();
        if value {
            styles.0 &= !LayoutStyle::SINGLE_LINE.0;
        } else {
            styles |= LayoutStyle::SINGLE_LINE;
        }
        self.text_break.set_layout_styles(styles);
        self.is_dirty = true;
    }
    pub fn enable_line_wrap(&mut self, value: bool) {
        if self.is_linewrap_enabled == value {
            return;
        }
        self.is_linewrap_enabled = value;
        self.text_break.set_line_width(if value {
            self.available_width
        } else {
            PAGE_WIDTH_MAX
        } as f32);
        self.is_dirty = true;
    }
    pub fn set_comb_text(&mut self, value: bool) {
        if self.is_comb_text == value {
            return;
        }
        self.is_comb_text = value;
        let mut styles = self.text_break.get_layout_styles();
        if value {
            styles |= LayoutStyle::COMB_TEXT;
            self.set_comb_text_width();
        } else {
            styles.0 &= !LayoutStyle::COMB_TEXT.0;
        }
        self.text_break.set_layout_styles(styles);
        self.is_dirty = true;
    }
    fn set_comb_text_width(&mut self) {
        let mut width = self.available_width;
        if self.has_character_limit {
            width = width.checked_div(self.character_limit).unwrap_or(0);
        }
        self.text_break.set_comb_width(width as f32);
    }
    pub fn layout(&mut self) {
        if !self.is_dirty {
            return;
        }
        self.is_dirty = false;
        self.rebuild_pieces();
    }
    pub fn get_contents_bounding_box(&mut self) -> CfxRectF {
        self.layout();
        self.contents_bounding_box
    }
    pub fn can_generate_character_info(&self) -> bool {
        self.text_length > 0 && self.font.is_some()
    }
    pub fn get_text_pieces(&mut self) -> &[FdeTextEditPiece] {
        self.layout();
        &self.text_piece_info
    }
    fn rebuild_pieces(&mut self) {
        self.text_break.reset();
        self.char_widths.clear();
        self.text_piece_info.clear();
        self.contents_bounding_box = CfxRectF::default();
        if !self.can_generate_character_info() {
            return;
        }
        let mut initialized = false;
        let mut piece_start = 0usize;
        let mut line_start = 0.;
        // Include the source's end-of-text sentinel, which supplies end-caret
        // geometry. Reading directly through the gap avoids cloning the text.
        for index in 0..=self.text_length {
            let ch = self.get_char(index);
            let mut status = self.text_break.append_char(ch);
            if index == self.text_length && break_type_none_or_piece(status) {
                status = self.text_break.end_break(BreakType::Paragraph);
            }
            if break_type_none_or_piece(status) {
                continue;
            }
            for i in 0..self.text_break.count_break_pieces() {
                let Some((piece, chars)) = self.text_break.get_break_piece(i) else {
                    continue;
                };
                let count = piece.get_length();
                let Ok(start) = i32::try_from(piece_start) else {
                    return;
                };
                let Ok(count_i32) = i32::try_from(count) else {
                    return;
                };
                let rect = CfxRectF::new(
                    piece.start_pos as f32 / 20_000.,
                    line_start,
                    piece.width as f32 / 20_000.,
                    self.line_spacing,
                );
                let mut styles = piece.char_styles;
                if piece.bidi_level & 1 != 0 {
                    styles |= FX_TXTCHARSTYLE_ODD_BIDI_LEVEL;
                }
                self.text_piece_info.push(FdeTextEditPiece {
                    rect,
                    start,
                    count: count_i32,
                    bidi_level: piece.bidi_level,
                    char_styles: styles,
                });
                if initialized {
                    self.contents_bounding_box.union(rect);
                } else {
                    self.contents_bounding_box = rect;
                    initialized = true;
                }
                piece_start += count;
                self.char_widths
                    .extend(piece.chars(chars).iter().map(|ch| ch.char_width));
            }
            line_start += self.line_spacing;
            self.text_break.clear_break_pieces();
        }
        let width = self.available_width as f32;
        let smaller = self.contents_bounding_box.width < width;
        let delta = if self.character_alignment & ALIGNMENT_RIGHT != 0 && smaller {
            width - self.contents_bounding_box.width
        } else if self.character_alignment & ALIGNMENT_CENTER != 0 && smaller {
            (width - self.contents_bounding_box.width) / 2.
        } else {
            0.
        };
        if delta != 0. {
            let offset = delta - self.contents_bounding_box.left;
            for piece in &mut self.text_piece_info {
                piece.rect.offset(offset, 0.);
            }
            self.contents_bounding_box.offset(offset, 0.);
        }
        if let Some(last_top) = self.text_piece_info.last().map(|piece| piece.rect.top) {
            self.contents_bounding_box.height -= self.line_spacing - self.font_size;
            // A directional run can be followed by the separate LTR end
            // sentinel. Every piece on this line must share the same baseline;
            // shrinking just the sentinel leaves the visible RTL run lower.
            for piece in self.text_piece_info.iter_mut().rev() {
                if piece.rect.top != last_top {
                    break;
                }
                piece.rect.height = self.font_size;
            }
        }
    }
    fn get_char_rects(&self, piece: &FdeTextEditPiece) -> Vec<CfxRectF> {
        if piece.count < 1 {
            return Vec::new();
        }
        let mut view = self.run_view();
        let mut run = TxtBreakRun {
            engine: Some(&mut view),
            start: piece.start,
            length: piece.count,
            font: self.font.clone(),
            font_size: self.font_size,
            styles: self.text_break.get_layout_styles(),
            char_styles: piece.char_styles,
            rect: piece.rect,
            ..Default::default()
        };
        self.text_break.get_char_rects(&mut run)
    }
    pub fn get_display_pos(&mut self, piece: &FdeTextEditPiece) -> Vec<TextCharPos> {
        self.layout();
        if piece.count < 1 {
            return Vec::new();
        }
        let mut view = self.run_view();
        let mut run = TxtBreakRun {
            engine: Some(&mut view),
            start: piece.start,
            length: piece.count,
            font: self.font.clone(),
            font_size: self.font_size,
            styles: self.text_break.get_layout_styles(),
            char_styles: piece.char_styles,
            rect: piece.rect,
            ..Default::default()
        };
        let count = self.text_break.get_display_pos(&mut run, &mut []);
        let mut positions = vec![TextCharPos::default(); count];
        self.text_break.get_display_pos(&mut run, &mut positions);
        positions
    }
    fn run_view(&self) -> TextEditRun<'_> {
        TextEditRun {
            content: &self.content,
            widths: &self.char_widths,
            gap_position: self.gap_position,
            gap_size: self.gap_size,
            length: self.text_length,
            password: self.password_mode,
            alias: self.password_alias,
        }
    }
    pub fn get_character_info(&mut self, start: i32) -> Option<(i32, CfxRectF)> {
        self.layout();
        if start < 0 || start as usize > self.text_length {
            return None;
        }
        let piece = self.text_piece_info.iter().find(|piece| {
            piece.start <= start && start < piece.start.saturating_add(piece.count)
        })?;
        Some((
            piece.bidi_level,
            *self
                .get_char_rects(piece)
                .get((start - piece.start) as usize)?,
        ))
    }
    pub fn get_character_rects_in_range(&mut self, start: i32, count: i32) -> Vec<CfxRectF> {
        self.layout();
        if start < 0 || count <= 0 {
            return Vec::new();
        }
        let end = start.saturating_add(count);
        let mut rects = Vec::new();
        for piece in &self.text_piece_info {
            let first = start.max(piece.start);
            let last = end.min(piece.start.saturating_add(piece.count));
            if first >= last {
                continue;
            }
            let chars = self.get_char_rects(piece);
            let Some(mut rect) = chars.get((first - piece.start) as usize).copied() else {
                continue;
            };
            for ch in chars
                .iter()
                .take((last - piece.start) as usize)
                .skip((first - piece.start + 1) as usize)
            {
                rect.union(*ch);
            }
            rects.push(rect);
        }
        rects
    }
    pub fn get_index_for_point(&mut self, point: CfxPointF) -> usize {
        self.layout();
        let mut row = self
            .text_piece_info
            .iter()
            .enumerate()
            .filter(|(_, piece)| piece.rect.top <= point.y && point.y < piece.rect.bottom());
        let Some((first_index, first)) = row.next() else {
            return self.text_length;
        };
        let mut left = first;
        let mut right = first;
        let mut right_index = first_index;
        for (index, piece) in row {
            if piece.rect.left < left.rect.left {
                left = piece;
            }
            if piece.rect.right() > right.rect.right() {
                right = piece;
                right_index = index;
            }
        }
        for piece in self
            .text_piece_info
            .iter()
            .filter(|piece| piece.rect.top <= point.y && point.y < piece.rect.bottom())
        {
            for (i, rect) in self.get_char_rects(piece).iter().enumerate() {
                if point.x < rect.left || point.x >= rect.right() {
                    continue;
                }
                let before = point.x - rect.left < rect.right() - point.x;
                let caret = if piece.bidi_level & 1 != 0 {
                    if before {
                        i + 1
                    } else {
                        i
                    }
                } else if before {
                    i
                } else {
                    i + 1
                };
                let mut position = (piece.start.max(0) as usize + caret).min(self.text_length);
                let ch = self.get_char(position);
                if ch == 10 && position > 0 && self.get_char(position - 1) == 13 {
                    position -= 1;
                }
                return position;
            }
        }
        // Test every piece on a visual row before falling back to its edge;
        // the source returned after the first piece, missing bidi fragments.
        if point.x < left.rect.left {
            return if left.bidi_level & 1 != 0 {
                (left.start.saturating_add(left.count).max(0) as usize).min(self.text_length)
            } else {
                left.start.max(0) as usize
            };
        }
        let mut position = if right.bidi_level & 1 != 0 {
            right.start.max(0) as usize
        } else {
            right.start.saturating_add(right.count).max(0) as usize
        };
        position = position.min(self.text_length);
        let last = right_index + 1 == self.text_piece_info.len();
        if !last
            && right.bidi_level & 1 == 0
            && position > 0
            && matches!(self.get_char(position - 1), 32 | 10 | 13)
        {
            position -= 1;
        }
        if position > 0 && self.get_char(position) == 10 && self.get_char(position - 1) == 13 {
            position -= 1;
        }
        position
    }
    pub fn bounds_for_word_at(&self, index: usize) -> (usize, usize) {
        if index > self.text_length {
            return (0, 0);
        }
        if index == self.text_length {
            return (index, 0);
        }
        let mut iter = TextEditIterator::new(self);
        iter.set_at(index);
        let start = iter.find_next_break_pos(true);
        let end = iter.find_next_break_pos(false);
        (start, end.saturating_sub(start).saturating_add(1))
    }
}
impl TxtBreakEngine for CfdeTextEditEngine {
    fn get_char(&self, index: usize) -> u32 {
        self.get_char(index)
    }
    fn get_width_of_char(&mut self, index: usize) -> i32 {
        self.get_width_of_char(index)
    }
}

/// A borrowed, already-laid-out run permits the breaker to query text and
/// widths without a mutable self-reference or copying the gap buffer.
struct TextEditRun<'a> {
    content: &'a [u32],
    widths: &'a [i32],
    gap_position: usize,
    gap_size: usize,
    length: usize,
    password: bool,
    alias: u32,
}
impl TxtBreakEngine for TextEditRun<'_> {
    fn get_char(&self, index: usize) -> u32 {
        if index >= self.length {
            return 0;
        }
        if self.password {
            return self.alias;
        }
        self.content[if index < self.gap_position {
            index
        } else {
            index + self.gap_size
        }]
    }
    fn get_width_of_char(&mut self, index: usize) -> i32 {
        self.widths.get(index).copied().unwrap_or(0)
    }
}

pub struct TextEditIterator<'a> {
    engine: &'a CfdeTextEditEngine,
    current_position: isize,
}
impl<'a> TextEditIterator<'a> {
    pub fn new(engine: &'a CfdeTextEditEngine) -> Self {
        Self {
            engine,
            current_position: -1,
        }
    }
    pub fn next(&mut self, previous: bool) {
        if self.is_eof(previous) {
            return;
        }
        if previous {
            self.current_position -= 1;
        } else {
            self.current_position += 1;
        }
    }
    pub fn get_char(&self) -> u32 {
        usize::try_from(self.current_position)
            .ok()
            .map_or(0, |index| self.engine.get_char(index))
    }
    pub fn set_at(&mut self, index: usize) {
        self.current_position =
            index.min(self.engine.text_length).min(isize::MAX as usize) as isize;
    }
    pub fn is_eof(&self, previous: bool) -> bool {
        if previous {
            self.current_position == -1
        } else {
            self.current_position >= 0 && self.current_position as usize == self.engine.text_length
        }
    }
    pub fn find_next_break_pos(&mut self, previous: bool) -> usize {
        if self.is_eof(previous) {
            return self.current_position.max(0) as usize;
        }
        let mut prior = WordBreakProperty::None;
        if !self.is_eof(!previous) {
            self.next(!previous);
            prior = get_word_break_property(self.get_char());
            self.next(previous);
        }
        let mut current = get_word_break_property(self.get_char());
        let mut first = true;
        while !self.is_eof(previous) {
            self.next(previous);
            let mut next = get_word_break_property(self.get_char());
            let mut word_break = check_state_change_for_word_break(current, next);
            if word_break {
                if self.is_eof(previous) {
                    self.next(!previous);
                    break;
                }
                if first {
                    let flags = get_break_flags_for(current, next);
                    if flags > 0 {
                        if break_flags_changed(flags, prior) {
                            self.next(!previous);
                            break;
                        }
                        self.next(previous);
                        word_break = false;
                    }
                }
                if word_break {
                    let flags = get_break_flags_for(next, current);
                    if flags <= 0 {
                        self.next(!previous);
                        break;
                    }
                    self.next(previous);
                    next = get_word_break_property(self.get_char());
                    if break_flags_changed(flags, next) {
                        self.next(!previous);
                        self.next(!previous);
                        break;
                    }
                }
            }
            current = next;
            first = false;
        }
        self.current_position.max(0) as usize
    }
}
fn get_break_flags_for(current: WordBreakProperty, next: WordBreakProperty) -> u8 {
    match (current, next) {
        (
            WordBreakProperty::MidLetter | WordBreakProperty::MidNumLet,
            WordBreakProperty::ALetter,
        ) => 1,
        (WordBreakProperty::MidNum | WordBreakProperty::MidNumLet, WordBreakProperty::Numeric) => 2,
        _ => 0,
    }
}
fn break_flags_changed(flags: u8, previous: WordBreakProperty) -> bool {
    (flags != 1 || previous != WordBreakProperty::ALetter)
        && (flags != 2 || previous != WordBreakProperty::Numeric)
}

#[cfg(test)]
#[path = "cfde_texteditengine_hit_tests.rs"]
mod hit_tests;

#[cfg(test)]
#[path = "cfde_texteditengine_sequence_tests.rs"]
mod sequence_tests;
