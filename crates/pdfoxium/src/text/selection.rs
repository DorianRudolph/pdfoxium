// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Copyright 2010 The Chromium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Loose selection bounds, following Chromium PDFiumRange's adjacent-run merge.
use super::TextSelection;
use crate::Rect;
use pdfoxium_core::{fpdftext::cpdf_textpage::CharType, fxcrt::fx_coordinates::CfxFloatRect};

#[derive(Clone, Copy)]
struct Run {
    rect: CfxFloatRect,
    count: usize,
    line: usize,
}
// Chromium's GetVerticalOverlap / ShouldMergeHorizontalRects: containment has
// full overlap; otherwise require 80% intersection/union height, and allow one
// average character width on each side. Compare original adjacent runs, not
// the growing union (which could incorrectly pull in other lines/columns).
fn should_merge(a: Run, b: Run) -> bool {
    if a.line != b.line {
        return false;
    }
    let height = a.rect.top.max(b.rect.top) - a.rect.bottom.min(b.rect.bottom);
    let overlap = if height == a.rect.height() || height == b.rect.height() {
        1.0
    } else {
        (a.rect.top.min(b.rect.top) - a.rect.bottom.max(b.rect.bottom)).max(0.0) / height
    };
    if overlap < 0.8 {
        return false;
    }
    let aw = a.rect.width() / a.count as f32;
    let bw = b.rect.width() / b.count as f32;
    a.rect.left - aw < b.rect.right + bw && a.rect.right + aw > b.rect.left - bw
}

pub(super) fn display_rects(selection: &TextSelection) -> Vec<Rect> {
    let core = &selection.text.0.core;
    let mut runs: Vec<Run> = Vec::new();
    let mut current: Option<Run> = None;
    let mut previous_char: Option<Run> = None;
    let mut object = None;
    let mut line = 0;
    for index in selection.chars.clone() {
        let info = core.get_char_info(index);
        if matches!(info.unicode(), 10 | 13) {
            if let Some(run) = current.take() {
                runs.push(run);
            }
            previous_char = None;
            object = None;
            line += 1;
            continue;
        }
        // Generated spacing has no font metrics. Neighboring real text runs
        // bridge that spacing via the same average-character-width rule.
        if info.char_type() == CharType::Generated {
            continue;
        }
        let mut rect = core.get_char_loose_bounds(index);
        rect.normalize();
        if rect.is_empty()
            || ![rect.left, rect.bottom, rect.right, rect.top]
                .iter()
                .all(|v| v.is_finite())
        {
            continue;
        }
        let next = Run {
            rect,
            count: 1,
            line,
        };
        // Existing text-object identities provide run boundaries. Also split
        // within an object at a spatial discontinuity (e.g. a large TJ gap).
        if object == info.text_object() && previous_char.is_some_and(|a| should_merge(a, next)) {
            let run = current.as_mut().expect("a previous character has a run");
            run.rect.union(rect);
            run.count += 1;
        } else {
            if let Some(run) = current.take() {
                runs.push(run);
            }
            current = Some(next);
        }
        previous_char = Some(next);
        object = info.text_object();
    }
    if let Some(run) = current {
        runs.push(run);
    }
    let mut rects: Vec<CfxFloatRect> = Vec::new();
    let mut previous = None;
    for run in runs {
        if previous.is_some_and(|a| should_merge(a, run)) {
            rects
                .last_mut()
                .expect("a previous run has bounds")
                .union(run.rect);
        } else {
            rects.push(run.rect);
        }
        previous = Some(run);
    }
    rects
        .into_iter()
        .map(|r| Rect {
            left: r.left,
            bottom: r.bottom,
            right: r.right,
            top: r.top,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(x: f32, y: f32, w: f32, h: f32, count: usize) -> Run {
        Run {
            rect: CfxFloatRect::new(x, y, x + w, y + h),
            count,
            line: 0,
        }
    }
    #[test]
    fn chromium_overlap_and_average_character_width_rules() {
        assert!(should_merge(
            run(0., 0., 20., 10., 4),
            run(25., 0., 20., 10., 4)
        ));
        assert!(!should_merge(
            run(0., 0., 20., 10., 4),
            run(40., 0., 20., 10., 4)
        ));
        assert!(should_merge(
            run(0., 0., 20., 10., 4),
            run(20., 1., 20., 10., 4)
        ));
        assert!(!should_merge(
            run(0., 0., 20., 10., 4),
            run(20., 3., 20., 10., 4)
        ));
        assert!(should_merge(
            run(0., 0., 20., 10., 4),
            run(20., 2., 5., 5., 1)
        ));
        let mut next = run(20., 0., 20., 10., 4);
        next.line = 1;
        assert!(!should_merge(run(0., 0., 20., 10., 4), next));
    }
}
