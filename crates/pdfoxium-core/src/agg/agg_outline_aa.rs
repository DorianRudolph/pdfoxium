// Adapted from PDFium's Anti-Grain Geometry 2.3 for pdfoxium.
// Distributed under the original permissive license notice below.
//
// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Cell accumulation and sorting from `agg_rasterizer_scanline_aa.{h,cpp}`.
//! Cells use a contiguous vector in place of AGG's 4096-entry blocks.

const POLY_BASE_SHIFT: i32 = 8;
const POLY_BASE_SIZE: i32 = 1 << POLY_BASE_SHIFT;
const POLY_BASE_MASK: i32 = POLY_BASE_SIZE - 1;
const CELL_BLOCK_LIMIT: usize = 1024 * 4096;
const DX_LIMIT: i32 = 16384 << POLY_BASE_SHIFT;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CellAa {
    pub x: i32,
    pub y: i32,
    pub cover: i32,
    pub area: i32,
}

impl CellAa {
    fn set(&mut self, x: i32, y: i32, cover: i32, area: i32) {
        *self = Self { x, y, cover, area };
    }

    fn set_cover(&mut self, cover: i32, area: i32) {
        self.cover = cover;
        self.area = area;
    }

    fn add_cover(&mut self, cover: i32, area: i32) {
        self.cover = self.cover.wrapping_add(cover);
        self.area = self.area.wrapping_add(area);
    }
}

// Cache the sort key alongside the cell index, avoiding a cell-buffer lookup
// and bounds check on every comparison. The cell block limit fits in u32.
// This is eight bytes, the same size as the former usize index on 64-bit hosts.
#[derive(Clone, Copy, Default)]
struct SortedCell {
    x: i32,
    index: u32,
}

#[derive(Clone, Copy, Default)]
struct SortedY {
    start: usize,
    num: usize,
}

pub struct OutlineAa {
    cells: Vec<CellAa>,
    sorted_cells: Vec<SortedCell>,
    sorted_y: Vec<SortedY>,
    current: CellAa,
    cur_x: i32,
    cur_y: i32,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    sorted: bool,
}

impl Default for OutlineAa {
    fn default() -> Self {
        let mut outline = Self {
            cells: Vec::new(),
            sorted_cells: Vec::new(),
            sorted_y: Vec::new(),
            current: CellAa::default(),
            cur_x: 0,
            cur_y: 0,
            min_x: i32::MAX,
            min_y: i32::MAX,
            max_x: -i32::MAX,
            max_y: -i32::MAX,
            sorted: false,
        };
        outline.reset();
        outline
    }
}

impl OutlineAa {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.cells.capacity() * std::mem::size_of::<CellAa>()
            + self.sorted_cells.capacity() * std::mem::size_of::<SortedCell>()
            + self.sorted_y.capacity() * std::mem::size_of::<SortedY>()
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.cells.clear();
        self.current.set(i32::MAX, i32::MAX, 0, 0);
        self.sorted = false;
        self.min_x = i32::MAX;
        self.min_y = i32::MAX;
        self.max_x = -i32::MAX;
        self.max_y = -i32::MAX;
    }

    pub fn min_x(&self) -> i32 {
        self.min_x
    }

    pub fn min_y(&self) -> i32 {
        self.min_y
    }

    pub fn max_x(&self) -> i32 {
        self.max_x
    }

    pub fn max_y(&self) -> i32 {
        self.max_y
    }

    pub fn total_cells(&self) -> usize {
        self.cells.len()
    }

    pub fn sorted(&self) -> bool {
        self.sorted
    }

    pub fn scanline_cells(&self, y: i32) -> impl Iterator<Item = CellAa> + '_ {
        let row = self.sorted_y[(i64::from(y) - i64::from(self.min_y)) as usize];
        self.sorted_cells[row.start..row.start + row.num]
            .iter()
            .map(|cell| self.cells[cell.index as usize])
    }

    fn add_cur_cell(&mut self) {
        if (self.current.area | self.current.cover) != 0 && self.cells.len() < CELL_BLOCK_LIMIT {
            self.cells.push(self.current);
        }
    }

    fn set_cur_cell(&mut self, x: i32, y: i32) {
        if self.current.x != x || self.current.y != y {
            self.add_cur_cell();
            self.current.set(x, y, 0, 0);
            self.min_x = self.min_x.min(x);
            self.max_x = self.max_x.max(x);
            self.min_y = self.min_y.min(y);
            self.max_y = self.max_y.max(y);
        }
    }

    fn render_hline(&mut self, ey: i32, x1: i32, mut y1: i32, x2: i32, y2: i32) {
        let mut ex1 = x1 >> POLY_BASE_SHIFT;
        let ex2 = x2 >> POLY_BASE_SHIFT;
        let fx1 = x1 & POLY_BASE_MASK;
        let fx2 = x2 & POLY_BASE_MASK;
        if y1 == y2 {
            self.set_cur_cell(ex2, ey);
            return;
        }
        if ex1 == ex2 {
            let delta = y2 - y1;
            self.current.add_cover(delta, (fx1 + fx2) * delta);
            return;
        }
        let mut p = (POLY_BASE_SIZE - fx1) * (y2 - y1);
        let mut first = POLY_BASE_SIZE;
        let mut incr = 1;
        let mut dx = x2 - x1;
        if dx < 0 {
            p = fx1 * (y2 - y1);
            first = 0;
            incr = -1;
            dx = -dx;
        }
        let mut delta = p / dx;
        let mut modulo = p % dx;
        if modulo < 0 {
            delta -= 1;
            modulo += dx;
        }
        self.current.add_cover(delta, (fx1 + first) * delta);
        ex1 += incr;
        self.set_cur_cell(ex1, ey);
        y1 += delta;
        if ex1 != ex2 {
            p = POLY_BASE_SIZE * (y2 - y1 + delta);
            let mut lift = p / dx;
            let mut rem = p % dx;
            if rem < 0 {
                lift -= 1;
                rem += dx;
            }
            modulo -= dx;
            while ex1 != ex2 {
                delta = lift;
                modulo += rem;
                if modulo >= 0 {
                    modulo -= dx;
                    delta += 1;
                }
                self.current.add_cover(delta, POLY_BASE_SIZE * delta);
                y1 += delta;
                ex1 += incr;
                self.set_cur_cell(ex1, ey);
            }
        }
        delta = y2 - y1;
        self.current
            .add_cover(delta, (fx2 + POLY_BASE_SIZE - first) * delta);
    }

    fn render_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let Some(dx) = x2.checked_sub(x1) else {
            return;
        };
        if dx >= DX_LIMIT || dx <= -DX_LIMIT {
            let Some(cx) = x1.checked_add(x2).map(|sum| sum / 2) else {
                return;
            };
            let Some(cy) = y1.checked_add(y2).map(|sum| sum / 2) else {
                return;
            };
            self.render_line(x1, y1, cx, cy);
            self.render_line(cx, cy, x2, y2);
            // PDFium continues through the original segment after subdivision.
        }
        let mut dy = y2.wrapping_sub(y1);
        let mut ey1 = y1 >> POLY_BASE_SHIFT;
        let ey2 = y2 >> POLY_BASE_SHIFT;
        let fy1 = y1 & POLY_BASE_MASK;
        let fy2 = y2 & POLY_BASE_MASK;
        if ey1 == ey2 {
            self.render_hline(ey1, x1, fy1, x2, fy2);
            return;
        }
        let mut incr = 1;
        if dx == 0 {
            let ex = x1 >> POLY_BASE_SHIFT;
            let two_fx = (x1 - (ex << POLY_BASE_SHIFT)) << 1;
            let mut first = POLY_BASE_SIZE;
            if dy < 0 {
                first = 0;
                incr = -1;
            }
            let delta = first - fy1;
            self.current.add_cover(delta, two_fx * delta);
            ey1 += incr;
            self.set_cur_cell(ex, ey1);
            let delta = first + first - POLY_BASE_SIZE;
            let area = two_fx * delta;
            while ey1 != ey2 {
                self.current.set_cover(delta, area);
                ey1 += incr;
                self.set_cur_cell(ex, ey1);
            }
            let delta = fy2 - POLY_BASE_SIZE + first;
            self.current.add_cover(delta, two_fx * delta);
            return;
        }
        let mut p = match (POLY_BASE_SIZE - fy1).checked_mul(dx) {
            Some(p) => p,
            None => return,
        };
        let mut first = POLY_BASE_SIZE;
        if dy < 0 {
            p = match fy1.checked_mul(dx) {
                Some(p) => p,
                None => return,
            };
            first = 0;
            incr = -1;
            dy = dy.wrapping_neg();
        }
        let mut delta = p / dy;
        let mut modulo = p % dy;
        if modulo < 0 {
            delta -= 1;
            modulo += dy;
        }
        let mut x_from = x1.wrapping_add(delta);
        self.render_hline(ey1, x1, fy1, x_from, first);
        ey1 += incr;
        self.set_cur_cell(x_from >> POLY_BASE_SHIFT, ey1);
        if ey1 != ey2 {
            p = match POLY_BASE_SIZE.checked_mul(dx) {
                Some(p) => p,
                None => return,
            };
            let mut lift = p / dy;
            let mut rem = p % dy;
            if rem < 0 {
                lift -= 1;
                rem += dy;
            }
            modulo -= dy;
            while ey1 != ey2 {
                delta = lift;
                modulo += rem;
                if modulo >= 0 {
                    modulo -= dy;
                    delta += 1;
                }
                let x_to = x_from.wrapping_add(delta);
                self.render_hline(ey1, x_from, POLY_BASE_SIZE - first, x_to, first);
                x_from = x_to;
                ey1 += incr;
                self.set_cur_cell(x_from >> POLY_BASE_SHIFT, ey1);
            }
        }
        self.render_hline(ey1, x_from, POLY_BASE_SIZE - first, x2, fy2);
    }

    pub fn move_to(&mut self, x: i32, y: i32) {
        if self.sorted {
            self.reset();
        }
        self.set_cur_cell(x >> POLY_BASE_SHIFT, y >> POLY_BASE_SHIFT);
        self.cur_x = x;
        self.cur_y = y;
    }

    pub fn line_to(&mut self, x: i32, y: i32) {
        self.render_line(self.cur_x, self.cur_y, x, y);
        self.cur_x = x;
        self.cur_y = y;
        self.sorted = false;
    }

    pub fn sort_cells(&mut self) -> bool {
        if self.sorted {
            return true;
        }
        self.add_cur_cell();
        if self.cells.is_empty() {
            return false;
        }
        let Some(height) = i64::from(self.max_y)
            .checked_sub(i64::from(self.min_y))
            .and_then(|span| span.checked_add(1))
            .and_then(|span| usize::try_from(span).ok())
        else {
            return false;
        };
        self.sorted_y.clear();
        if self.sorted_y.try_reserve(height).is_err() {
            return false;
        }
        self.sorted_y.resize(height, SortedY::default());
        for cell in &self.cells {
            let row = (i64::from(cell.y) - i64::from(self.min_y)) as usize;
            self.sorted_y[row].start += 1;
        }
        let mut start = 0;
        for row in &mut self.sorted_y {
            let count = row.start;
            row.start = start;
            start += count;
        }
        self.sorted_cells.clear();
        if self.sorted_cells.try_reserve(self.cells.len()).is_err() {
            return false;
        }
        self.sorted_cells
            .resize(self.cells.len(), SortedCell::default());
        for (index, cell) in self.cells.iter().enumerate() {
            let row = (i64::from(cell.y) - i64::from(self.min_y)) as usize;
            let sorted_row = &mut self.sorted_y[row];
            self.sorted_cells[sorted_row.start + sorted_row.num] = SortedCell {
                x: cell.x,
                index: index as u32,
            };
            sorted_row.num += 1;
        }
        for row in &self.sorted_y {
            self.sorted_cells[row.start..row.start + row.num].sort_unstable_by_key(|cell| cell.x);
        }
        self.sorted = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_x_sort_retains_cell_order_including_equal_keys() {
        let mut random = 1u32;
        for count in (1..=128).chain([255, 256, 257, 1024]) {
            for pattern in 0..5 {
                let mut outline = OutlineAa::new();
                outline.min_y = -2;
                outline.max_y = 1;
                for index in 0..count {
                    random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                    let x = match pattern {
                        0 => index as i32,
                        1 => -(index as i32),
                        2 => 7,
                        3 => (random % 9) as i32 - 4,
                        _ => random as i32,
                    };
                    outline.cells.push(CellAa {
                        x,
                        y: (index % 4) as i32 - 2,
                        cover: index as i32,
                        area: random as i32,
                    });
                }
                let expected: Vec<Vec<_>> = (-2..=1)
                    .map(|y| {
                        let mut indices: Vec<_> =
                            (0..count).filter(|&i| outline.cells[i].y == y).collect();
                        indices.sort_unstable_by_key(|&i| outline.cells[i].x);
                        indices.into_iter().map(|i| outline.cells[i]).collect()
                    })
                    .collect();
                assert!(outline.sort_cells());
                for (y, expected) in (-2..=1).zip(expected) {
                    assert_eq!(
                        outline.scanline_cells(y).collect::<Vec<_>>(),
                        expected,
                        "count={count}, pattern={pattern}, y={y}"
                    );
                }
            }
        }
        assert!(CELL_BLOCK_LIMIT <= u32::MAX as usize);
    }

    #[test]
    fn vertical_edge_accumulates_coverage_across_rows() {
        let mut outline = OutlineAa::new();
        outline.move_to(2 * POLY_BASE_SIZE, 0);
        outline.line_to(2 * POLY_BASE_SIZE, 3 * POLY_BASE_SIZE);
        assert!(outline.sort_cells());
        assert_eq!((outline.min_x(), outline.max_x()), (2, 2));
        assert_eq!((outline.min_y(), outline.max_y()), (0, 3));
        let rows: Vec<_> = (0..3)
            .map(|y| outline.scanline_cells(y).collect::<Vec<_>>())
            .collect();
        assert_eq!(rows.iter().map(Vec::len).collect::<Vec<_>>(), vec![1, 1, 1]);
        assert!(rows.iter().all(|row| row[0].cover == POLY_BASE_SIZE));
    }
}
