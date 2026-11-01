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

//! Gray8 instantiation of `third_party/agg23/agg_renderer_base.h`.

use super::agg_basics::Rect;
use super::agg_color_gray::Gray8;
use super::agg_pixfmt_gray::PixfmtGray8;

pub struct RendererBaseGray<'p, 'r, 'b> {
    pixel_format: Option<&'p mut PixfmtGray8<'r, 'b>>,
    clip_box: Rect,
}

impl<'p, 'r, 'b> Default for RendererBaseGray<'p, 'r, 'b> {
    fn default() -> Self {
        Self {
            pixel_format: None,
            clip_box: Rect::new(1, 1, 0, 0),
        }
    }
}

impl<'p, 'r, 'b> RendererBaseGray<'p, 'r, 'b> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pixel_format(pixel_format: &'p mut PixfmtGray8<'r, 'b>) -> Self {
        let mut renderer = Self::new();
        renderer.attach(pixel_format);
        renderer
    }

    pub fn attach(&mut self, pixel_format: &'p mut PixfmtGray8<'r, 'b>) {
        self.clip_box = Rect::new(
            0,
            0,
            pixel_format.width().wrapping_sub(1) as i32,
            pixel_format.height().wrapping_sub(1) as i32,
        );
        self.pixel_format = Some(pixel_format);
    }

    pub fn pixel_format(&self) -> &PixfmtGray8<'r, 'b> {
        self.pixel_format.as_deref().expect("renderer not attached")
    }

    pub fn pixel_format_mut(&mut self) -> &mut PixfmtGray8<'r, 'b> {
        self.pixel_format
            .as_deref_mut()
            .expect("renderer not attached")
    }

    pub fn width(&self) -> u32 {
        self.pixel_format().width()
    }

    pub fn height(&self) -> u32 {
        self.pixel_format().height()
    }

    pub fn first_clip_box(&mut self) {}

    pub fn next_clip_box(&self) -> bool {
        false
    }

    pub fn clip_box(&self) -> Rect {
        self.clip_box
    }

    pub fn xmin(&self) -> i32 {
        self.clip_box.x1
    }

    pub fn ymin(&self) -> i32 {
        self.clip_box.y1
    }

    pub fn xmax(&self) -> i32 {
        self.clip_box.x2
    }

    pub fn ymax(&self) -> i32 {
        self.clip_box.y2
    }

    pub fn bounding_clip_box(&self) -> Rect {
        self.clip_box
    }

    pub fn bounding_xmin(&self) -> i32 {
        self.xmin()
    }

    pub fn bounding_ymin(&self) -> i32 {
        self.ymin()
    }

    pub fn bounding_xmax(&self) -> i32 {
        self.xmax()
    }

    pub fn bounding_ymax(&self) -> i32 {
        self.ymax()
    }

    pub fn blend_hline(&mut self, mut x1: i32, y: i32, mut x2: i32, color: Gray8, cover: u8) {
        if x1 > x2 {
            std::mem::swap(&mut x1, &mut x2);
        }
        if y > self.ymax() || y < self.ymin() || x1 > self.xmax() || x2 < self.xmin() {
            return;
        }
        x1 = x1.max(self.xmin());
        x2 = x2.min(self.xmax());
        self.pixel_format_mut().blend_hline(
            x1,
            y,
            (i64::from(x2) - i64::from(x1) + 1) as u32,
            color,
            cover,
        );
    }

    pub fn blend_solid_hspan(&mut self, mut x: i32, y: i32, len: i32, color: Gray8, covers: &[u8]) {
        if y > self.ymax() || y < self.ymin() {
            return;
        }
        let mut len = i64::from(len);
        let mut cover_offset = 0usize;
        if x < self.xmin() {
            let shift = i64::from(self.xmin()) - i64::from(x);
            len -= shift;
            if len <= 0 {
                return;
            }
            cover_offset = shift as usize;
            x = self.xmin();
        }
        if i64::from(x) + len > i64::from(self.xmax()) {
            len = i64::from(self.xmax()) - i64::from(x) + 1;
            if len <= 0 {
                return;
            }
        }
        let covers = covers
            .get(cover_offset..cover_offset + len as usize)
            .expect("coverage span too short");
        self.pixel_format_mut()
            .blend_solid_hspan(x, y, color, covers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agg::agg_rendering_buffer::RenderingBuffer;

    #[test]
    fn horizontal_lines_and_spans_clip_to_bitmap() {
        let mut bytes = [0u8; 8];
        let mut buffer = RenderingBuffer::from_buffer(&mut bytes, 4, 2, 4).unwrap();
        let mut pixels = PixfmtGray8::new(&mut buffer);
        let mut renderer = RendererBaseGray::with_pixel_format(&mut pixels);
        assert_eq!(renderer.clip_box(), Rect::new(0, 0, 3, 1));
        renderer.blend_hline(-2, 0, 1, Gray8::opaque(100), 255);
        renderer.blend_solid_hspan(2, 1, 4, Gray8::opaque(200), &[255, 255, 255, 255]);
        assert_eq!(renderer.pixel_format().pixel(0, 0).unwrap().v, 100);
        assert_eq!(renderer.pixel_format().pixel(2, 0).unwrap().v, 0);
        assert_eq!(renderer.pixel_format().pixel(2, 1).unwrap().v, 200);
        assert_eq!(renderer.pixel_format().pixel(3, 1).unwrap().v, 200);
    }
}
