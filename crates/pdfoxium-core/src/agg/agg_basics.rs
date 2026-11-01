// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_basics.h` for the AGG renderer.

pub type Int8 = i8;
pub type Int8u = u8;
pub type Int16 = i16;
pub type Int16u = u16;
pub type Int32 = i32;
pub type Int32u = u32;
pub type Int64 = i64;
pub type Int64u = u64;
pub type CoverType = u8;

pub const COVER_SHIFT: u32 = 8;
pub const COVER_SIZE: u32 = 1 << COVER_SHIFT;
pub const COVER_MASK: u32 = COVER_SIZE - 1;
pub const COVER_NONE: u32 = 0;
pub const COVER_FULL: u32 = COVER_MASK;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RectBase<T> {
    pub x1: T,
    pub y1: T,
    pub x2: T,
    pub y2: T,
}

impl<T: Copy + PartialOrd> RectBase<T> {
    pub const fn new(x1: T, y1: T, x2: T, y2: T) -> Self {
        Self { x1, y1, x2, y2 }
    }

    pub fn normalize(&mut self) -> &Self {
        if self.x1 > self.x2 {
            std::mem::swap(&mut self.x1, &mut self.x2);
        }
        if self.y1 > self.y2 {
            std::mem::swap(&mut self.y1, &mut self.y2);
        }
        self
    }

    pub fn clip(&mut self, rect: &Self) -> bool {
        if self.x2 > rect.x2 {
            self.x2 = rect.x2;
        }
        if self.y2 > rect.y2 {
            self.y2 = rect.y2;
        }
        if self.x1 < rect.x1 {
            self.x1 = rect.x1;
        }
        if self.y1 < rect.y1 {
            self.y1 = rect.y1;
        }
        self.is_valid()
    }

    pub fn is_valid(&self) -> bool {
        self.x1 <= self.x2 && self.y1 <= self.y2
    }
}

pub type Rect = RectBase<i32>;
pub type RectD = RectBase<f32>;

pub fn intersect_rectangles<T: Copy + PartialOrd>(
    mut first: RectBase<T>,
    second: RectBase<T>,
) -> RectBase<T> {
    first.clip(&second);
    first
}

pub fn unite_rectangles<T: Copy + PartialOrd>(
    mut first: RectBase<T>,
    second: RectBase<T>,
) -> RectBase<T> {
    if first.x2 < second.x2 {
        first.x2 = second.x2;
    }
    if first.y2 < second.y2 {
        first.y2 = second.y2;
    }
    if first.x1 > second.x1 {
        first.x1 = second.x1;
    }
    if first.y1 > second.y1 {
        first.y1 = second.y1;
    }
    first
}

pub const PATH_CMD_STOP: u32 = 0;
pub const PATH_CMD_MOVE_TO: u32 = 1;
pub const PATH_CMD_LINE_TO: u32 = 2;
pub const PATH_CMD_CURVE3: u32 = 3;
pub const PATH_CMD_CURVE4: u32 = 4;
pub const PATH_CMD_CURVEN: u32 = 5;
pub const PATH_CMD_CATROM: u32 = 6;
pub const PATH_CMD_UBSPLINE: u32 = 7;
pub const PATH_CMD_END_POLY: u32 = 0x0f;
pub const PATH_CMD_MASK: u32 = 0x0f;
pub const PATH_FLAGS_NONE: u32 = 0;
pub const PATH_FLAGS_CCW: u32 = 0x10;
pub const PATH_FLAGS_CW: u32 = 0x20;
pub const PATH_FLAGS_CLOSE: u32 = 0x40;
pub const PATH_FLAGS_JR: u32 = 0x80;
pub const PATH_FLAGS_MASK: u32 = 0xf0;

fn without_jr(command: u32) -> u32 {
    command & !PATH_FLAGS_JR
}

pub fn is_vertex(command: u32) -> bool {
    (PATH_CMD_MOVE_TO..PATH_CMD_END_POLY).contains(&without_jr(command))
}
pub fn is_drawing(command: u32) -> bool {
    (PATH_CMD_LINE_TO..PATH_CMD_END_POLY).contains(&without_jr(command))
}
pub fn is_stop(command: u32) -> bool {
    without_jr(command) == PATH_CMD_STOP
}
pub fn is_move_to(command: u32) -> bool {
    without_jr(command) == PATH_CMD_MOVE_TO
}
pub fn is_line_to(command: u32) -> bool {
    without_jr(command) == PATH_CMD_LINE_TO
}
pub fn is_curve(command: u32) -> bool {
    matches!(without_jr(command), PATH_CMD_CURVE3 | PATH_CMD_CURVE4)
}
pub fn is_curve3(command: u32) -> bool {
    without_jr(command) == PATH_CMD_CURVE3
}
pub fn is_curve4(command: u32) -> bool {
    without_jr(command) == PATH_CMD_CURVE4
}
pub fn is_end_poly(command: u32) -> bool {
    without_jr(command) & PATH_CMD_MASK == PATH_CMD_END_POLY
}
pub fn is_close(command: u32) -> bool {
    without_jr(command) & !(PATH_FLAGS_CW | PATH_FLAGS_CCW)
        == (PATH_CMD_END_POLY | PATH_FLAGS_CLOSE)
}
pub fn is_next_poly(command: u32) -> bool {
    is_stop(command) || is_move_to(command) || is_end_poly(command)
}
pub fn is_cw(command: u32) -> bool {
    without_jr(command) & PATH_FLAGS_CW != 0
}
pub fn is_ccw(command: u32) -> bool {
    without_jr(command) & PATH_FLAGS_CCW != 0
}
pub fn is_oriented(command: u32) -> bool {
    without_jr(command) & (PATH_FLAGS_CW | PATH_FLAGS_CCW) != 0
}
pub fn is_closed(command: u32) -> bool {
    without_jr(command) & PATH_FLAGS_CLOSE != 0
}
pub fn get_close_flag(command: u32) -> u32 {
    without_jr(command) & PATH_FLAGS_CLOSE
}
pub fn clear_orientation(command: u32) -> u32 {
    without_jr(command) & !(PATH_FLAGS_CW | PATH_FLAGS_CCW)
}
pub fn get_orientation(command: u32) -> u32 {
    without_jr(command) & (PATH_FLAGS_CW | PATH_FLAGS_CCW)
}
pub fn set_orientation(command: u32, orientation: u32) -> u32 {
    clear_orientation(command) | orientation
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointType {
    pub x: f32,
    pub y: f32,
    pub flag: u32,
}

impl PointType {
    pub const fn new(x: f32, y: f32, flag: u32) -> Self {
        Self { x, y, flag }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VertexType {
    pub x: f32,
    pub y: f32,
    pub cmd: u32,
}

impl VertexType {
    pub const fn new(x: f32, y: f32, cmd: u32) -> Self {
        Self { x, y, cmd }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_and_command_rules_match_agg() {
        let mut rect = Rect::new(5, 7, 1, 2);
        rect.normalize();
        assert_eq!(rect, Rect::new(1, 2, 5, 7));
        assert!(rect.clip(&Rect::new(3, 4, 8, 9)));
        assert_eq!(rect, Rect::new(3, 4, 5, 7));
        let command = PATH_CMD_END_POLY | PATH_FLAGS_CLOSE | PATH_FLAGS_CW | PATH_FLAGS_JR;
        assert!(is_end_poly(command));
        assert!(is_close(command));
        assert!(is_cw(command));
        assert_eq!(get_orientation(command), PATH_FLAGS_CW);
        assert_eq!(
            clear_orientation(command),
            PATH_CMD_END_POLY | PATH_FLAGS_CLOSE
        );
    }
}
