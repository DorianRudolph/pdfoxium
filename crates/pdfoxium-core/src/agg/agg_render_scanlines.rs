// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_render_scanlines.h`.

use super::agg_scanline_u::ScanlineU8;

pub trait ScanlineRasterizer {
    fn rewind_scanlines(&mut self) -> bool;
    fn min_x(&self) -> i32;
    fn max_x(&self) -> i32;
    fn sweep_scanline(&mut self, scanline: &mut ScanlineU8, no_smooth: bool) -> bool;
}

pub trait ScanlineRenderer {
    /// Existing consumers retain expanded coverage unless they explicitly
    /// support the single-byte coverage of a solid ScanlineSpan.
    fn supports_solid_spans(&self) -> bool {
        false
    }
    fn prepare(&mut self, max_span_len: u32);
    fn render(&mut self, scanline: &ScanlineU8);
}

/// Returns false if the scanline coverage allocation cannot be made.
pub fn render_scanlines<R: ScanlineRasterizer, T: ScanlineRenderer>(
    rasterizer: &mut R,
    scanline: &mut ScanlineU8,
    renderer: &mut T,
    no_smooth: bool,
) -> bool {
    if rasterizer.rewind_scanlines() {
        let reset = if renderer.supports_solid_spans() {
            scanline.reset_with_solid_spans(rasterizer.min_x(), rasterizer.max_x())
        } else {
            scanline.reset(rasterizer.min_x(), rasterizer.max_x())
        };
        if !reset {
            return false;
        }
        renderer
            .prepare((rasterizer.max_x().wrapping_sub(rasterizer.min_x()) as u32).wrapping_add(2));
        while rasterizer.sweep_scanline(scanline, no_smooth) {
            renderer.render(scanline);
        }
    }
    true
}

/// The upstream helper calls `render_scanlines()` without its required
/// `no_smooth` parameter. It has no PDFium caller; the callable Rust version
/// supplies that parameter explicitly.
pub trait PathRasterizer<S, Id>: ScanlineRasterizer {
    fn reset(&mut self);
    fn add_path(&mut self, source: &mut S, path_id: Id);
}

pub trait ColoredScanlineRenderer<Color>: ScanlineRenderer {
    fn color(&mut self, color: Color);
}

pub fn render_all_paths<R, T, S, Color: Copy, Id: Copy>(
    rasterizer: &mut R,
    scanline: &mut ScanlineU8,
    renderer: &mut T,
    source: &mut S,
    colors: &[Color],
    path_ids: &[Id],
    num_paths: usize,
    no_smooth: bool,
) -> bool
where
    R: PathRasterizer<S, Id>,
    T: ColoredScanlineRenderer<Color>,
{
    for index in 0..num_paths {
        let color = colors[index];
        let path_id = path_ids[index];
        rasterizer.reset();
        rasterizer.add_path(source, path_id);
        renderer.color(color);
        if !render_scanlines(rasterizer, scanline, renderer, no_smooth) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod solid_span_tests {
    use super::*;
    struct Runs(bool);
    impl ScanlineRasterizer for Runs {
        fn rewind_scanlines(&mut self) -> bool {
            self.0 = false;
            true
        }
        fn min_x(&self) -> i32 {
            0
        }
        fn max_x(&self) -> i32 {
            255
        }
        fn sweep_scanline(&mut self, row: &mut ScanlineU8, _: bool) -> bool {
            if self.0 {
                return false;
            }
            self.0 = true;
            row.reset_spans();
            row.add_span(0, 256, 128);
            row.finalize(0);
            true
        }
    }
    struct Expanded;
    impl ScanlineRenderer for Expanded {
        fn prepare(&mut self, _: u32) {}
        fn render(&mut self, row: &ScanlineU8) {
            let span = row.spans().next().unwrap();
            assert!(!span.solid);
            assert_eq!(span.covers, &[128; 256]);
        }
    }
    struct Compact;
    impl ScanlineRenderer for Compact {
        fn supports_solid_spans(&self) -> bool {
            true
        }
        fn prepare(&mut self, _: u32) {}
        fn render(&mut self, row: &ScanlineU8) {
            let span = row.spans().next().unwrap();
            assert!(span.solid);
            assert_eq!((span.len, span.covers), (256, &[128][..]));
        }
    }
    #[test]
    fn compact_coverage_requires_renderer_opt_in() {
        let mut row = ScanlineU8::new();
        assert!(render_scanlines(
            &mut Runs(false),
            &mut row,
            &mut Expanded,
            false
        ));
        assert!(render_scanlines(
            &mut Runs(false),
            &mut row,
            &mut Compact,
            false
        ));
        assert!(render_scanlines(
            &mut Runs(false),
            &mut row,
            &mut Expanded,
            false
        ));
    }
}
