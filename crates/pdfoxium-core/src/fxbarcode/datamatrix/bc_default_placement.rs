// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code is licensed as follows:
/*
 * Copyright 2006 Jeremias Maerki.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

pub struct CbcDefaultPlacement {
    codewords: Vec<u8>,
    numrows: i32,
    numcols: i32,
    bits: Vec<u8>,
}
impl CbcDefaultPlacement {
    pub fn new(codewords: Vec<u8>, numcols: usize, numrows: usize) -> Option<Self> {
        if numcols == 0 || numrows == 0 {
            return None;
        }
        let size = numcols.checked_mul(numrows)?;
        let numcols = i32::try_from(numcols).ok()?;
        let numrows = i32::try_from(numrows).ok()?;
        numcols.checked_add(4)?;
        numrows.checked_add(4)?;
        let mut bits = Vec::new();
        bits.try_reserve_exact(size).ok()?;
        bits.resize(size, 2);
        let mut placement = Self {
            codewords,
            numrows,
            numcols,
            bits,
        };
        placement.init()?;
        Some(placement)
    }
    fn index(&self, col: i32, row: i32) -> Option<usize> {
        if col < 0 || row < 0 || col >= self.numcols || row >= self.numrows {
            return None;
        }
        Some(row as usize * self.numcols as usize + col as usize)
    }
    pub fn get_bit(&self, col: usize, row: usize) -> bool {
        let (Ok(col), Ok(row)) = (i32::try_from(col), i32::try_from(row)) else {
            return false;
        };
        self.index(col, row).is_some_and(|i| self.bits[i] == 1)
    }
    fn set_bit(&mut self, col: i32, row: i32, bit: bool) -> Option<()> {
        let index = self.index(col, row)?;
        self.bits[index] = u8::from(bit);
        Some(())
    }
    fn has_bit(&self, col: i32, row: i32) -> Option<bool> {
        Some(self.bits[self.index(col, row)?] != 2)
    }
    fn init(&mut self) -> Option<()> {
        let mut pos = 0;
        let mut row = 4;
        let mut col = 0;
        loop {
            if row == self.numrows && col == 0 {
                self.set_corner1(pos)?;
                pos += 1;
            }
            if row == self.numrows - 2 && col == 0 && self.numcols % 4 != 0 {
                self.set_corner2(pos)?;
                pos += 1;
            }
            if row == self.numrows - 2 && col == 0 && self.numcols % 8 == 4 {
                self.set_corner3(pos)?;
                pos += 1;
            }
            if row == self.numrows + 4 && col == 2 && self.numcols % 8 == 0 {
                self.set_corner4(pos)?;
                pos += 1;
            }
            loop {
                if row < self.numrows && col >= 0 && !self.has_bit(col, row)? {
                    self.set_utah(row, col, pos)?;
                    pos += 1;
                }
                row -= 2;
                col += 2;
                if row < 0 || col >= self.numcols {
                    break;
                }
            }
            row += 1;
            col += 3;
            loop {
                if row >= 0 && col < self.numcols && !self.has_bit(col, row)? {
                    self.set_utah(row, col, pos)?;
                    pos += 1;
                }
                row += 2;
                col -= 2;
                if row >= self.numrows || col < 0 {
                    break;
                }
            }
            row += 3;
            col += 1;
            if row >= self.numrows && col >= self.numcols {
                break;
            }
        }
        if !self.has_bit(self.numcols - 1, self.numrows - 1)? {
            self.set_bit(self.numcols - 1, self.numrows - 1, true)?;
            self.set_bit(self.numcols - 2, self.numrows - 2, true)?;
        }
        Some(())
    }
    fn set_module(&mut self, mut row: i32, mut col: i32, pos: usize, bit: u32) -> Option<()> {
        if row < 0 {
            row += self.numrows;
            col += 4 - ((self.numrows + 4) % 8);
        }
        if col < 0 {
            col += self.numcols;
            row += 4 - ((self.numcols + 4) % 8);
        }
        let value = *self.codewords.get(pos)? & (1 << (8 - bit));
        self.set_bit(col, row, value != 0)
    }
    fn set_utah(&mut self, row: i32, col: i32, pos: usize) -> Option<()> {
        let cells = [
            (row - 2, col - 2),
            (row - 2, col - 1),
            (row - 1, col - 2),
            (row - 1, col - 1),
            (row - 1, col),
            (row, col - 2),
            (row, col - 1),
            (row, col),
        ];
        self.set_cells(&cells, pos)
    }
    fn set_cells(&mut self, cells: &[(i32, i32); 8], pos: usize) -> Option<()> {
        for (i, &(row, col)) in cells.iter().enumerate() {
            self.set_module(row, col, pos, (i + 1) as u32)?;
        }
        Some(())
    }
    fn set_corner1(&mut self, pos: usize) -> Option<()> {
        let r = self.numrows;
        let c = self.numcols;
        self.set_cells(
            &[
                (r - 1, 0),
                (r - 1, 1),
                (r - 1, 2),
                (0, c - 2),
                (0, c - 1),
                (1, c - 1),
                (2, c - 1),
                (3, c - 1),
            ],
            pos,
        )
    }
    fn set_corner2(&mut self, pos: usize) -> Option<()> {
        let r = self.numrows;
        let c = self.numcols;
        self.set_cells(
            &[
                (r - 3, 0),
                (r - 2, 0),
                (r - 1, 0),
                (0, c - 4),
                (0, c - 3),
                (0, c - 2),
                (0, c - 1),
                (1, c - 1),
            ],
            pos,
        )
    }
    fn set_corner3(&mut self, pos: usize) -> Option<()> {
        let r = self.numrows;
        let c = self.numcols;
        self.set_cells(
            &[
                (r - 3, 0),
                (r - 2, 0),
                (r - 1, 0),
                (0, c - 2),
                (0, c - 1),
                (1, c - 1),
                (2, c - 1),
                (3, c - 1),
            ],
            pos,
        )
    }
    fn set_corner4(&mut self, pos: usize) -> Option<()> {
        let r = self.numrows;
        let c = self.numcols;
        self.set_cells(
            &[
                (r - 1, 0),
                (r - 1, c - 1),
                (0, c - 3),
                (0, c - 2),
                (0, c - 1),
                (1, c - 3),
                (1, c - 2),
                (1, c - 1),
            ],
            pos,
        )
    }
}
