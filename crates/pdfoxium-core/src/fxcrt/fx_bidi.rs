// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Directional segments and line-order heuristic from `core/fxcrt/fx_bidi`.
use super::fx_unicode::{get_bidi_class, BidiClass};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Direction {
    #[default]
    Neutral,
    Left,
    Right,
    LeftWeak,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BidiSegment {
    pub start: usize,
    pub count: usize,
    pub direction: Direction,
}
#[derive(Debug, Default)]
pub struct CfxBidiChar {
    current: BidiSegment,
    last: BidiSegment,
}
impl CfxBidiChar {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn append_char(&mut self, character: u32) -> bool {
        let direction = match get_bidi_class(character) {
            BidiClass::L => Direction::Left,
            BidiClass::AN
            | BidiClass::EN
            | BidiClass::NSM
            | BidiClass::CS
            | BidiClass::ES
            | BidiClass::ET
            | BidiClass::BN => Direction::LeftWeak,
            BidiClass::R | BidiClass::AL => Direction::Right,
            _ => Direction::Neutral,
        };
        let changed = direction != self.current.direction;
        if changed {
            self.start_new_segment(direction);
        }
        self.current.count += 1;
        changed
    }
    pub fn end_char(&mut self) -> bool {
        self.start_new_segment(Direction::Neutral);
        self.last.count > 0
    }
    pub fn get_segment_info(&self) -> &BidiSegment {
        &self.last
    }
    fn start_new_segment(&mut self, direction: Direction) {
        self.last = self.current;
        self.current = BidiSegment {
            start: self.current.start + self.current.count,
            count: 0,
            direction,
        };
    }
}
#[derive(Debug)]
pub struct CfxBidiString {
    order: Vec<BidiSegment>,
    direction: Direction,
}
impl CfxBidiString {
    pub fn new(text: &[u32], auto_order: bool) -> Self {
        let mut bidi = CfxBidiChar::new();
        let mut order = Vec::new();
        for &character in text {
            if bidi.append_char(character) {
                order.push(*bidi.get_segment_info());
            }
        }
        if bidi.end_char() {
            order.push(*bidi.get_segment_info());
        }
        let mut result = Self {
            order,
            direction: Direction::Left,
        };
        if auto_order {
            let right = result
                .order
                .iter()
                .filter(|segment| segment.direction == Direction::Right)
                .count();
            let left = result
                .order
                .iter()
                .filter(|segment| segment.direction == Direction::Left)
                .count();
            if right > left {
                result.set_overall_direction_right();
            }
        }
        result
    }
    pub fn overall_direction(&self) -> Direction {
        self.direction
    }
    pub fn set_overall_direction_right(&mut self) {
        if self.direction != Direction::Right {
            self.order.reverse();
            self.direction = Direction::Right;
        }
    }
    pub fn segments(&self) -> &[BidiSegment] {
        &self.order
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incremental_segments_preserve_initial_empty_boundary_and_repeated_end() {
        let mut bidi = CfxBidiChar::new();
        assert_eq!(*bidi.get_segment_info(), BidiSegment::default());
        assert!(!bidi.end_char());
        for (middle, direction) in [
            (32, Direction::Neutral),
            (46, Direction::LeftWeak),
            (1488, Direction::Right),
        ] {
            let mut bidi = CfxBidiChar::new();
            assert!(bidi.append_char(65));
            assert_eq!(*bidi.get_segment_info(), BidiSegment::default());
            assert!(!bidi.append_char(65));
            assert!(!bidi.append_char(65));
            assert!(bidi.append_char(middle));
            assert_eq!(
                *bidi.get_segment_info(),
                BidiSegment {
                    start: 0,
                    count: 3,
                    direction: Direction::Left
                }
            );
            for _ in 0..3 {
                assert!(!bidi.append_char(middle));
            }
            assert!(bidi.append_char(66));
            assert_eq!(
                *bidi.get_segment_info(),
                BidiSegment {
                    start: 3,
                    count: 4,
                    direction
                }
            );
            assert!(bidi.end_char());
            assert_eq!(
                *bidi.get_segment_info(),
                BidiSegment {
                    start: 7,
                    count: 1,
                    direction: Direction::Left
                }
            );
            assert!(!bidi.end_char());
        }
    }
    #[test]
    fn heuristic_counts_strong_segments_and_forced_reversal_is_idempotent() {
        let text = [1488, 32, 65, 32, 1488];
        let logical = CfxBidiString::new(&text, false);
        let mut automatic = CfxBidiString::new(&text, true);
        assert_eq!(logical.overall_direction(), Direction::Left);
        assert_eq!(automatic.overall_direction(), Direction::Right);
        assert!(logical
            .segments()
            .iter()
            .eq(automatic.segments().iter().rev()));
        let before = automatic.segments().to_vec();
        automatic.set_overall_direction_right();
        assert_eq!(automatic.segments(), before);
        let tie = CfxBidiString::new(&[65, 1488], true);
        assert_eq!(tie.overall_direction(), Direction::Left);
        let weak = CfxBidiString::new(&[46, 46], true);
        assert_eq!(weak.overall_direction(), Direction::Left);
        assert_eq!(
            weak.segments(),
            &[
                BidiSegment::default(),
                BidiSegment {
                    start: 0,
                    count: 2,
                    direction: Direction::LeftWeak
                }
            ]
        );
        assert!(CfxBidiString::new(&[], true).segments().is_empty());
    }
}
