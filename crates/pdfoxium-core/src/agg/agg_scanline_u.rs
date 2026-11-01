// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Safe gray8 scanline storage from `third_party/agg23/agg_scanline_u.h`.

const MIN_SOLID_SPAN_LEN: u32 = 128;

#[derive(Clone, Copy, Debug)]
struct Span {
    x: i16,
    len: i16,
    cover_offset: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct ScanlineSpan<'a> {
    pub x: i16,
    pub len: i16,
    pub covers: &'a [u8],
    /// A single coverage byte describes the entire positive-length span.
    pub solid: bool,
}

impl ScanlineSpan<'_> {
    #[inline]
    pub fn cover_at(&self, index: usize) -> u8 {
        self.covers[if self.solid { 0 } else { index }]
    }
}

pub struct ScanlineU8 {
    min_x: i32,
    max_len: usize,
    last_x: i32,
    y: i32,
    covers: Vec<u8>,
    spans: Vec<Span>,
    compact: bool,
}

impl Default for ScanlineU8 {
    fn default() -> Self {
        Self {
            min_x: 0,
            max_len: 0,
            last_x: 0x7fff_fff0,
            y: 0,
            covers: Vec::new(),
            spans: Vec::new(),
            compact: false,
        }
    }
}

impl ScanlineU8 {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.covers.capacity() + self.spans.capacity() * std::mem::size_of::<Span>()
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self, min_x: i32, max_x: i32) -> bool {
        self.compact = false;
        let Some(max_len) = i64::from(max_x)
            .checked_sub(i64::from(min_x))
            .and_then(|length| length.checked_add(2))
            .and_then(|length| usize::try_from(length).ok())
        else {
            return false;
        };
        if max_len > self.max_len {
            if self
                .covers
                .try_reserve_exact(max_len - self.covers.len())
                .is_err()
            {
                return false;
            }
            if self
                .spans
                .try_reserve_exact(max_len - self.spans.len())
                .is_err()
            {
                return false;
            }
            self.covers.resize(max_len, 0);
            self.max_len = max_len;
        }
        self.min_x = min_x;
        self.reset_spans();
        true
    }

    pub(crate) fn reset_with_solid_spans(&mut self, min_x: i32, max_x: i32) -> bool {
        if !self.reset(min_x, max_x) {
            return false;
        }
        // The legacy representation wraps i16 lengths/coordinates. Retain it
        // verbatim for large ranges so packing cannot change those edge cases.
        // Narrow paths cannot contain a packable run. Keep them expanded so
        // their edge cells do not pay for checking the preceding span's kind.
        let max_len = i64::from(max_x) - i64::from(min_x) + 2;
        self.compact = max_len > i64::from(MIN_SOLID_SPAN_LEN)
            && max_len <= i64::from(i16::MAX)
            && i16::try_from(min_x).is_ok()
            && i16::try_from(max_x).is_ok();
        true
    }

    pub fn add_cell(&mut self, x: i32, cover: u32) {
        let offset =
            usize::try_from(i64::from(x) - i64::from(self.min_x)).expect("cell before min x");
        self.covers[offset] = cover as u8;
        if x == self.last_x.wrapping_add(1)
            && (!self.compact || self.spans.last().is_some_and(|span| span.len > 0))
        {
            let span = self.spans.last_mut().expect("missing span");
            span.len = span.len.wrapping_add(1);
        } else {
            self.spans.push(Span {
                x: x as i16,
                len: 1,
                cover_offset: offset,
            });
        }
        self.last_x = x;
    }

    pub fn add_cells(&mut self, x: i32, covers: &[u8]) {
        if covers.is_empty() {
            return;
        }
        let offset =
            usize::try_from(i64::from(x) - i64::from(self.min_x)).expect("cells before min x");
        self.covers[offset..offset + covers.len()].copy_from_slice(covers);
        if x == self.last_x.wrapping_add(1)
            && (!self.compact || self.spans.last().is_some_and(|span| span.len > 0))
        {
            let span = self.spans.last_mut().expect("missing span");
            span.len = span.len.wrapping_add(covers.len() as i16);
        } else {
            self.spans.push(Span {
                x: x as i16,
                len: covers.len() as i16,
                cover_offset: offset,
            });
        }
        self.last_x = x.wrapping_add(covers.len() as i32).wrapping_sub(1);
    }

    pub fn add_span(&mut self, x: i32, len: u32, cover: u32) {
        if len == 0 {
            return;
        }
        let offset =
            usize::try_from(i64::from(x) - i64::from(self.min_x)).expect("span before min x");
        if self.compact && len >= MIN_SOLID_SPAN_LEN {
            // Keep short runs with their edge cells: splitting them costs more
            // than writing their coverage. Negative internal lengths encode
            // long solid runs without enlarging Span or adding a buffer.
            assert!(offset + len as usize <= self.covers.len());
            self.covers[offset] = cover as u8;
            if let Some(span) = self.spans.last_mut().filter(|span| {
                x == self.last_x.wrapping_add(1)
                    && span.len < 0
                    && self.covers[span.cover_offset] == cover as u8
            }) {
                span.len -= len as i16;
            } else {
                self.spans.push(Span {
                    x: x as i16,
                    len: -(len as i16),
                    cover_offset: offset,
                });
            }
            self.last_x = x.wrapping_add(len as i32).wrapping_sub(1);
            return;
        }
        self.covers[offset..offset + len as usize].fill(cover as u8);
        if x == self.last_x.wrapping_add(1)
            && (!self.compact || self.spans.last().is_some_and(|span| span.len > 0))
        {
            let span = self.spans.last_mut().expect("missing span");
            span.len = span.len.wrapping_add(len as i16);
        } else {
            self.spans.push(Span {
                x: x as i16,
                len: len as i16,
                cover_offset: offset,
            });
        }
        self.last_x = x.wrapping_add(len as i32).wrapping_sub(1);
    }

    pub fn finalize(&mut self, y: i32) {
        self.y = y;
    }

    pub fn reset_spans(&mut self) {
        self.last_x = 0x7fff_fff0;
        self.spans.clear();
    }

    pub fn y(&self) -> i32 {
        self.y
    }

    pub fn num_spans(&self) -> usize {
        self.spans.len()
    }

    pub fn spans(&self) -> impl Iterator<Item = ScanlineSpan<'_>> {
        self.spans.iter().map(|span| {
            let solid = self.compact && span.len < 0;
            let len = if solid { -span.len } else { span.len };
            let cover_count = if solid { 1 } else { len as u16 as usize };
            ScanlineSpan {
                x: span.x,
                len,
                covers: &self.covers[span.cover_offset..span.cover_offset + cover_count],
                solid,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_ranges_keep_expanded_mode_when_reusing_wide_storage() {
        let mut row = ScanlineU8::new();
        assert!(row.reset_with_solid_spans(-2, 1000));
        assert!(row.compact);
        let retained = row.retained_bytes();
        for len in [1, 17, 34, 64, 127, 128, 129] {
            assert!(row.reset_with_solid_spans(-2, len as i32 - 3));
            assert_eq!(row.compact, len >= MIN_SOLID_SPAN_LEN);
            row.add_span(-2, len, 137);
            let span = row.spans().next().unwrap();
            assert_eq!((span.x, span.len), (-2, len as i16));
            assert_eq!(span.solid, len >= MIN_SOLID_SPAN_LEN);
            assert!((0..len).all(|i| span.cover_at(i as usize) == 137));
            assert_eq!(row.retained_bytes(), retained);
        }
    }

    #[test]
    fn compact_runs_preserve_expanded_coverage_and_retained_capacity() {
        let mut plain = ScanlineU8::new();
        let mut compact = ScanlineU8::new();
        assert!(plain.reset(-2, 1000));
        assert!(compact.reset_with_solid_spans(-2, 1000));
        for row in [&mut plain, &mut compact] {
            row.add_cell(-2, 1);
            row.add_span(-1, 127, 17);
            row.add_span(126, 128, 254);
            row.add_span(254, 129, 254);
            row.add_cell(383, 3);
            row.add_cells(384, &[4, 5]);
            row.add_span(386, 140, 0);
            row.add_span(531, 300, 255);
            row.finalize(17);
        }
        let expand = |row: &ScanlineU8| {
            row.spans()
                .flat_map(|span| {
                    (0..span.len as usize)
                        .map(move |index| (i32::from(span.x) + index as i32, span.cover_at(index)))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(expand(&compact), expand(&plain));
        assert_eq!(compact.retained_bytes(), plain.retained_bytes());
        assert_eq!(compact.y(), plain.y());
        let solid: Vec<_> = compact
            .spans()
            .filter(|span| span.solid)
            .map(|span| (span.x, span.len, span.covers.to_vec()))
            .collect();
        assert_eq!(
            solid,
            [
                (126, 257, vec![254]),
                (386, 140, vec![0]),
                (531, 300, vec![255])
            ]
        );
        compact.reset_spans();
        compact.add_span(0, 128, 1);
        assert!(compact.spans().next().unwrap().solid);
        assert!(compact.reset(0, 250));
        compact.add_span(0, 128, 1);
        assert!(!compact.spans().next().unwrap().solid);
    }

    #[test]
    fn compact_mode_keeps_legacy_length_and_coordinate_wrapping() {
        for (min, max, count) in [(0, 32770, 32768), (40000, 40200, 128)] {
            let mut row = ScanlineU8::new();
            assert!(row.reset_with_solid_spans(min, max));
            row.add_span(min, count, 255);
            let span = row.spans().next().unwrap();
            assert!(!span.solid);
            assert_eq!(span.x, min as i16);
            assert_eq!(span.len, count as i16);
            assert_eq!(span.covers, vec![255; count as usize]);
        }
    }

    #[test]
    fn joins_adjacent_cells_and_retains_cover_offsets() {
        let mut scanline = ScanlineU8::new();
        assert!(scanline.reset(-2, 5));
        scanline.add_cell(-2, 10);
        scanline.add_cells(-1, &[20, 30]);
        scanline.add_span(3, 2, 40);
        scanline.finalize(7);
        assert_eq!(scanline.y(), 7);
        assert_eq!(scanline.num_spans(), 2);
        let spans: Vec<_> = scanline.spans().collect();
        assert_eq!(
            (spans[0].x, spans[0].len, spans[0].covers),
            (-2, 3, &[10, 20, 30][..])
        );
        assert_eq!(
            (spans[1].x, spans[1].len, spans[1].covers),
            (3, 2, &[40, 40][..])
        );
        scanline.reset_spans();
        assert_eq!(scanline.num_spans(), 0);
    }
}
