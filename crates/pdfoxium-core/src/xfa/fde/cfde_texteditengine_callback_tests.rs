// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Delegate contracts from CFDE_TextEditEngine's Insert/Delete/Replace paths.
//! These callbacks are an internal Rust entry point: CFWL_Edit handles host
//! notifications separately and does not install a TextEditDelegate.

use super::cfde_texteditengine::{
    CfdeTextEditEngine, RecordOperation, TextChange, TextEditDelegate,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn seeded(text: &str) -> CfdeTextEditEngine {
    let mut engine = CfdeTextEditEngine::new();
    engine.insert(0, &wide(text), RecordOperation::SkipRecord);
    engine
}
fn text(engine: &CfdeTextEditEngine, expected: &str) {
    assert_eq!(engine.get_text(), wide(expected));
}

#[derive(Debug, PartialEq)]
enum Event {
    Will {
        before: Vec<u32>,
        text: Vec<u32>,
        start: usize,
        end: usize,
    },
    Validate(Vec<u32>),
    Changed(Vec<u32>),
    Full(Vec<u32>),
}
type Will = Box<dyn Fn(&mut CfdeTextEditEngine, &mut TextChange)>;
type Validate = Box<dyn Fn(&mut CfdeTextEditEngine, &[u32]) -> bool>;
struct Delegate {
    will: Option<Will>,
    validate: Option<Validate>,
    events: RefCell<Vec<Event>>,
}
impl Delegate {
    fn new(will: Option<Will>, validate: Option<Validate>) -> Rc<Self> {
        Rc::new(Self {
            will,
            validate,
            events: RefCell::new(Vec::new()),
        })
    }
}
impl TextEditDelegate for Delegate {
    fn on_text_will_change(&self, engine: &mut CfdeTextEditEngine, change: &mut TextChange) {
        self.events.borrow_mut().push(Event::Will {
            before: change.previous_text.clone(),
            text: change.text.clone(),
            start: change.selection_start,
            end: change.selection_end,
        });
        if let Some(will) = &self.will {
            will(engine, change);
        }
    }
    fn on_validate(&self, engine: &mut CfdeTextEditEngine, value: &[u32]) -> bool {
        self.events
            .borrow_mut()
            .push(Event::Validate(value.to_vec()));
        self.validate
            .as_ref()
            .map_or(true, |validate| validate(engine, value))
    }
    fn on_text_changed(&self, engine: &mut CfdeTextEditEngine) {
        self.events
            .borrow_mut()
            .push(Event::Changed(engine.get_text()));
    }
    fn notify_text_full(&self, engine: &mut CfdeTextEditEngine) {
        self.events
            .borrow_mut()
            .push(Event::Full(engine.get_text()));
    }
}
fn round_trip(engine: &mut CfdeTextEditEngine, before: &str, after: &str) {
    text(engine, after);
    assert!(engine.can_undo());
    assert!(!engine.can_redo());
    assert!(engine.undo());
    text(engine, before);
    assert!(!engine.can_undo());
    assert!(engine.can_redo());
    assert!(engine.redo());
    text(engine, after);
    assert!(!engine.can_redo());
}

#[test]
fn cancelling_insert_preserves_selection_and_the_existing_redo_branch() {
    let mut engine = seeded("ABCDE");
    engine.insert(5, &wide("!"), RecordOperation::InsertRecord);
    assert!(engine.undo());
    engine.set_selection(1, 3);
    let delegate = Delegate::new(Some(Box::new(|_, change| change.cancelled = true)), None);
    engine.set_delegate(Some(delegate.clone()));
    engine.insert(2, &wide("discard"), RecordOperation::InsertRecord);
    text(&engine, "ABCDE");
    assert_eq!(engine.get_selection(), (1, 3));
    assert!(!engine.can_undo());
    assert!(engine.can_redo());
    assert_eq!(
        &*delegate.events.borrow(),
        &[Event::Will {
            before: wide("ABCDE"),
            text: wide("discard"),
            start: 2,
            end: 2
        }]
    );
    assert!(engine.redo());
    text(&engine, "ABCDE!");
}

#[test]
fn cancelling_delete_leaves_the_selected_range_and_history_untouched() {
    let mut engine = seeded("ABCDE");
    engine.set_selection(1, 3);
    let delegate = Delegate::new(Some(Box::new(|_, change| change.cancelled = true)), None);
    engine.set_delegate(Some(delegate.clone()));
    assert!(engine
        .delete(2, 2, RecordOperation::InsertRecord)
        .is_empty());
    text(&engine, "ABCDE");
    assert_eq!(engine.get_selection(), (1, 3));
    assert!(!engine.can_undo());
    assert_eq!(
        &*delegate.events.borrow(),
        &[Event::Will {
            before: wide("ABCDE"),
            text: Vec::new(),
            start: 2,
            end: 4
        }]
    );
}

#[test]
fn cancelling_selected_replacement_neither_erases_text_nor_creates_a_record() {
    let mut engine = seeded("ABCDE");
    engine.set_selection(1, 3);
    let delegate = Delegate::new(Some(Box::new(|_, change| change.cancelled = true)), None);
    engine.set_delegate(Some(delegate.clone()));
    engine.replace_selected_text(&wide("XY"));
    text(&engine, "ABCDE");
    assert_eq!(engine.get_selection(), (1, 3));
    assert!(!engine.can_undo());
    assert_eq!(
        &*delegate.events.borrow(),
        &[Event::Will {
            before: wide("ABCDE"),
            text: wide("XY"),
            start: 1,
            end: 4
        }]
    );
}

#[test]
fn callback_expanding_an_insert_range_records_the_complete_replacement() {
    let mut engine = seeded("ABCDE");
    // The callback's range, rather than the engine's unrelated selection,
    // identifies the content that must be restored by undo.
    engine.set_selection(0, 1);
    let delegate = Delegate::new(
        Some(Box::new(|_, change| {
            change.selection_start = 1;
            change.selection_end = 4;
            change.text = wide("XY");
        })),
        None,
    );
    engine.set_delegate(Some(delegate.clone()));
    engine.insert(2, &wide("requested"), RecordOperation::InsertRecord);
    assert_eq!(engine.history_validation_text(false), Some(wide("ABCDE")));
    round_trip(&mut engine, "ABCDE", "AXYE");
    assert_eq!(
        delegate
            .events
            .borrow()
            .iter()
            .filter(|e| matches!(e, Event::Will { .. }))
            .count(),
        1
    );
}

#[test]
fn callback_turning_an_insert_into_a_deletion_retains_undoable_removed_text() {
    let mut engine = seeded("ABCDE");
    let delegate = Delegate::new(
        Some(Box::new(|_, change| {
            change.selection_start = 1;
            change.selection_end = 4;
            change.text.clear();
        })),
        None,
    );
    engine.set_delegate(Some(delegate));
    engine.insert(2, &wide("requested"), RecordOperation::InsertRecord);
    round_trip(&mut engine, "ABCDE", "AE");
}

#[test]
fn validation_veto_after_insert_range_rewrite_preserves_original_text_and_selection() {
    let mut engine = seeded("ABCDE");
    engine.set_selection(0, 1);
    engine.enable_validation(true);
    let delegate = Delegate::new(
        Some(Box::new(|_, change| {
            change.selection_start = 1;
            change.selection_end = 4;
            change.text = wide("XY");
        })),
        Some(Box::new(|_, candidate| {
            assert_eq!(candidate, wide("AXYE"));
            false
        })),
    );
    engine.set_delegate(Some(delegate.clone()));
    engine.insert(2, &wide("requested"), RecordOperation::InsertRecord);
    text(&engine, "ABCDE");
    assert_eq!(engine.get_selection(), (0, 1));
    assert!(!engine.can_undo());
    assert!(!delegate
        .events
        .borrow()
        .iter()
        .any(|e| matches!(e, Event::Changed(_) | Event::Full(_))));
}

#[test]
fn callback_rewriting_only_a_delete_range_preserves_the_actual_removed_slice() {
    let mut engine = seeded("ABCDEFG");
    let delegate = Delegate::new(
        Some(Box::new(|_, change| {
            change.selection_start = 1;
            change.selection_end = 6;
        })),
        None,
    );
    engine.set_delegate(Some(delegate));
    assert_eq!(
        engine.delete(2, 1, RecordOperation::InsertRecord),
        wide("BCDEF")
    );
    round_trip(&mut engine, "ABCDEFG", "AG");
}

#[test]
fn delete_callback_replacement_is_one_reversible_edit_in_both_directions() {
    let mut engine = seeded("ABCDE");
    let delegate = Delegate::new(
        Some(Box::new(|_, change| {
            change.selection_start = 1;
            change.selection_end = 4;
            change.text = wide("XY");
        })),
        None,
    );
    engine.set_delegate(Some(delegate.clone()));
    assert_eq!(
        engine.delete(2, 1, RecordOperation::InsertRecord),
        wide("BCD")
    );
    assert_eq!(engine.history_validation_text(false), Some(wide("ABCDE")));
    round_trip(&mut engine, "ABCDE", "AXYE");
    assert_eq!(
        delegate
            .events
            .borrow()
            .iter()
            .filter(|e| matches!(e, Event::Will { .. }))
            .count(),
        1
    );
}

#[test]
fn delete_callback_replacement_history_uses_only_the_character_limit_prefix() {
    let mut engine = seeded("ABCDE");
    engine.set_has_character_limit(true);
    engine.set_character_limit(6);
    let delegate = Delegate::new(
        Some(Box::new(|_, change| {
            change.selection_start = 1;
            change.selection_end = 4;
            change.text = wide("WXYZ123");
        })),
        None,
    );
    engine.set_delegate(Some(delegate.clone()));
    assert_eq!(
        engine.delete(2, 1, RecordOperation::InsertRecord),
        wide("BCD")
    );
    assert!(engine.last_insert_was_full());
    assert!(delegate
        .events
        .borrow()
        .iter()
        .any(|e| *e == Event::Full(wide("AWXYZE"))));
    round_trip(&mut engine, "ABCDE", "AWXYZE");
}

#[test]
fn delete_callback_can_shrink_the_buffer_without_leaving_an_invalid_requested_range() {
    let mut engine = seeded("ABCDE");
    let delegate = Delegate::new(
        Some(Box::new(|engine, change| {
            engine.delete(0, 4, RecordOperation::SkipRecord);
            change.selection_start = 99;
            change.selection_end = 100;
        })),
        None,
    );
    engine.set_delegate(Some(delegate));
    assert!(engine
        .delete(2, 1, RecordOperation::InsertRecord)
        .is_empty());
    text(&engine, "E");
    assert!(!engine.can_undo());
}

#[test]
fn replacement_callback_can_redirect_selection_and_change_the_requested_value() {
    let mut engine = seeded("ABCDE");
    engine.set_selection(0, 1);
    let delegate = Delegate::new(
        Some(Box::new(|_, change| {
            change.selection_start = 1;
            change.selection_end = 4;
            change.text = wide("XY");
        })),
        None,
    );
    engine.set_delegate(Some(delegate));
    engine.replace_selected_text(&wide("requested"));
    assert_eq!(engine.history_validation_text(false), Some(wide("ABCDE")));
    assert!(engine.undo());
    text(&engine, "ABCDE");
    assert_eq!(engine.history_validation_text(true), Some(wide("AXYE")));
    assert!(engine.redo());
    text(&engine, "AXYE");
}

#[test]
fn rejected_selected_replacement_is_atomic_and_keeps_the_selection() {
    let mut engine = seeded("ABCDE");
    engine.set_selection(1, 3);
    engine.enable_validation(true);
    let delegate = Delegate::new(
        None,
        Some(Box::new(|_, candidate| {
            assert_eq!(candidate, wide("AXYE"));
            false
        })),
    );
    engine.set_delegate(Some(delegate.clone()));
    engine.replace_selected_text(&wide("XY"));
    text(&engine, "ABCDE");
    assert_eq!(engine.get_selection(), (1, 3));
    assert!(!engine.can_undo());
    assert!(!engine.can_redo());
    assert!(!delegate
        .events
        .borrow()
        .iter()
        .any(|e| matches!(e, Event::Changed(_) | Event::Full(_))));
}

#[test]
fn validation_reentry_recomputes_the_pending_index_after_the_host_shrinks_text() {
    let mut engine = seeded("ABCDE");
    engine.enable_validation(true);
    let entered = Rc::new(Cell::new(false));
    let once = entered.clone();
    let delegate = Delegate::new(
        None,
        Some(Box::new(move |engine, _| {
            if !once.replace(true) {
                engine.delete(0, 4, RecordOperation::SkipRecord);
            }
            true
        })),
    );
    engine.set_delegate(Some(delegate));
    engine.insert(5, &wide("XY"), RecordOperation::InsertRecord);
    assert!(entered.get());
    // The host's deletion is intentional and is not part of the outer record.
    round_trip(&mut engine, "E", "EXY");
}

#[test]
fn validation_reentry_rechecks_capacity_and_records_only_the_committed_prefix() {
    let mut engine = seeded("AB");
    engine.set_has_character_limit(true);
    engine.set_character_limit(5);
    engine.enable_validation(true);
    let entered = Rc::new(Cell::new(false));
    let once = entered.clone();
    let delegate = Delegate::new(
        None,
        Some(Box::new(move |engine, _| {
            if !once.replace(true) {
                engine.insert(engine.get_length(), &wide("Z"), RecordOperation::SkipRecord);
            }
            true
        })),
    );
    engine.set_delegate(Some(delegate.clone()));
    engine.insert(2, &wide("CDE"), RecordOperation::InsertRecord);
    text(&engine, "ABCDZ");
    assert!(engine.last_insert_was_full());
    let events = delegate.events.borrow();
    assert_eq!(
        events
            .iter()
            .filter_map(|e| if let Event::Validate(v) = e {
                Some(v.clone())
            } else {
                None
            })
            .collect::<Vec<_>>(),
        vec![wide("ABCDE"), wide("ABZ")]
    );
    let full = events
        .iter()
        .position(|e| *e == Event::Full(wide("ABCDZ")))
        .unwrap();
    let changed = events
        .iter()
        .rposition(|e| *e == Event::Changed(wide("ABCDZ")))
        .unwrap();
    assert!(full < changed);
    drop(events);
    round_trip(&mut engine, "ABZ", "ABCDZ");
}

#[test]
fn replacement_history_validation_veto_keeps_content_and_both_history_cursors() {
    let mut engine = seeded("ABCDE");
    engine.set_selection(1, 3);
    engine.replace_selected_text(&wide("XY"));
    engine.enable_validation(true);
    let allowed = Rc::new(Cell::new(false));
    let decision = allowed.clone();
    let delegate = Delegate::new(None, Some(Box::new(move |_, _| decision.get())));
    engine.set_delegate(Some(delegate));
    assert!(!engine.undo());
    text(&engine, "AXYE");
    assert!(engine.can_undo());
    assert!(!engine.can_redo());
    assert_eq!(engine.history_validation_text(false), Some(wide("ABCDE")));
    allowed.set(true);
    assert!(engine.undo());
    text(&engine, "ABCDE");
    allowed.set(false);
    assert!(!engine.redo());
    text(&engine, "ABCDE");
    assert!(!engine.can_undo());
    assert!(engine.can_redo());
    assert_eq!(engine.history_validation_text(true), Some(wide("AXYE")));
    allowed.set(true);
    assert!(engine.redo());
    text(&engine, "AXYE");
}

#[test]
fn rejected_history_action_preserves_intentional_host_edits_outside_the_pending_range() {
    let mut engine = seeded("ABCDE");
    engine.set_selection(1, 3);
    engine.replace_selected_text(&wide("XY"));
    engine.enable_validation(true);
    let entered = Rc::new(Cell::new(false));
    let once = entered.clone();
    let delegate = Delegate::new(
        None,
        Some(Box::new(move |engine, _| {
            if !once.replace(true) {
                engine.insert(engine.get_length(), &wide("!"), RecordOperation::SkipRecord);
                false
            } else {
                true
            }
        })),
    );
    engine.set_delegate(Some(delegate));
    assert!(!engine.undo());
    text(&engine, "AXYE!");
    assert!(engine.can_undo());
    assert!(!engine.can_redo());
    assert_eq!(engine.history_validation_text(false), Some(wide("ABCDE!")));
    assert!(engine.undo());
    text(&engine, "ABCDE!");
    assert!(engine.redo());
    text(&engine, "AXYE!");
}

#[test]
fn validation_reentry_that_applies_history_does_not_apply_the_outer_record_twice() {
    let mut engine = seeded("ABCDE");
    engine.set_selection(1, 3);
    engine.replace_selected_text(&wide("XY"));
    engine.enable_validation(true);
    let entered = Rc::new(Cell::new(false));
    let once = entered.clone();
    let delegate = Delegate::new(
        None,
        Some(Box::new(move |engine, _| {
            if !once.replace(true) {
                assert!(engine.undo());
            }
            true
        })),
    );
    engine.set_delegate(Some(delegate));
    // The host's nested undo succeeds. The outer request observes that its
    // record was already consumed and must preserve the host's history cursor.
    assert!(!engine.undo());
    text(&engine, "ABCDE");
    assert!(!engine.can_undo());
    assert!(engine.can_redo());
    engine.set_delegate(None);
    assert!(engine.redo());
    text(&engine, "AXYE");
}
