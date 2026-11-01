// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Fixed-seed edit traces checked against a simple Unicode-scalar snapshot log.
//! The model stores document states, not the engine's gap/ring representation.
use super::*;

struct Rng(u32);
impl Rng {
    fn take(&mut self, limit: usize) -> usize {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 as usize % limit
    }
}
struct Model {
    states: Vec<Vec<u32>>,
    at: usize,
}
impl Model {
    fn new(text: Vec<u32>) -> Self {
        Self {
            states: vec![text],
            at: 0,
        }
    }
    fn text(&self) -> &[u32] {
        &self.states[self.at]
    }
    fn edit(&mut self, start: usize, remove: usize, added: &[u32]) {
        let mut next = self.text().to_vec();
        next.splice(start..start + remove, added.iter().copied());
        assert_ne!(next, self.text());
        self.states.truncate(self.at + 1);
        self.states.push(next);
        self.at += 1;
    }
    fn check(&self, engine: &CfdeTextEditEngine, seed: u32, step: usize) {
        assert_eq!(engine.get_text(), self.text(), "seed={seed:x} step={step}");
        assert_eq!(engine.get_length(), self.text().len());
        assert_eq!(engine.can_undo(), self.at != 0);
        assert_eq!(engine.can_redo(), self.at + 1 != self.states.len());
    }
}
fn scalar_text(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn run(seed: u32) {
    let initial = scalar_text("AéΩ\n🙂Z");
    let mut engine = CfdeTextEditEngine::new();
    engine.insert(0, &initial, RecordOperation::SkipRecord);
    let mut model = Model::new(initial);
    let mut rng = Rng(seed);
    let words = ["β", "é🙂", "Ω\nZ", "ab", "\n🙂Q"];
    model.check(&engine, seed, 0);
    for step in 0..96 {
        let len = model.text().len();
        match step % 12 {
            2 | 5 | 8 => {
                assert!(engine.undo());
                model.at -= 1;
            }
            3 | 9 => {
                assert!(engine.redo());
                model.at += 1;
            }
            4 | 10 => {
                let start = if step % 12 == 10 { 0 } else { rng.take(len) };
                let remove = if step % 12 == 10 {
                    len
                } else {
                    1 + rng.take(len - start)
                };
                let expected_removed = model.text()[start..start + remove].to_vec();
                assert_eq!(
                    engine.delete(start, remove, RecordOperation::InsertRecord),
                    expected_removed
                );
                model.edit(start, remove, &[]);
            }
            1 | 7 => {
                let start = if step % 12 == 7 { 0 } else { rng.take(len) };
                let remove = if step % 12 == 7 {
                    len
                } else {
                    1 + rng.take(len - start)
                };
                let mut added = scalar_text(words[rng.take(words.len())]);
                if model.text()[start..start + remove] == added {
                    added.push('X' as u32);
                }
                engine.set_selection(start, remove);
                assert_eq!(
                    engine.get_selected_text(),
                    &model.text()[start..start + remove]
                );
                engine.replace_selected_text(&added);
                model.edit(start, remove, &added);
            }
            _ => {
                let start = rng.take(len + 1);
                let added = scalar_text(words[rng.take(words.len())]);
                engine.insert(start, &added, RecordOperation::InsertRecord);
                model.edit(start, 0, &added);
            }
        }
        model.check(&engine, seed, step);
    }
    // Consume and replay the surviving branch. Total records stay below the
    // separately tested 128-slot ring boundary; this model needs no ring logic.
    while model.at > 0 {
        assert!(engine.undo());
        model.at -= 1;
        model.check(&engine, seed, 96);
    }
    assert!(!engine.undo());
    while model.at + 1 < model.states.len() {
        assert!(engine.redo());
        model.at += 1;
        model.check(&engine, seed, 97);
    }
    assert!(!engine.redo());
}
#[test]
fn scalar_sequence_seed_00000001() {
    run(1);
}
#[test]
fn scalar_sequence_seed_13579bdf() {
    run(0x13579bdf);
}
#[test]
fn scalar_sequence_seed_2468ace0() {
    run(0x2468ace0);
}
#[test]
fn scalar_sequence_seed_5eed1234() {
    run(0x5eed1234);
}
#[test]
fn scalar_sequence_seed_7fffffff() {
    run(0x7fffffff);
}
#[test]
fn scalar_sequence_seed_80000000() {
    run(0x80000000);
}
#[test]
fn scalar_sequence_seed_cafebabe() {
    run(0xcafebabe);
}
#[test]
fn scalar_sequence_seed_ffffffff() {
    run(0xffffffff);
}
