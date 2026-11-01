// Interfaces copied from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::backend::document::DocumentState;
use crate::prelude::*;
use pdfoxium_core::{
    fpdfapi::font::cpdf_font::CpdfFont,
    fpdfsdk::fpdf_edittext as sdk,
    fxge::cfx_path::{PathPoint, PointType},
};
use std::{
    cell::OnceCell,
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub enum PdfPathSegmentType {
    Unknown = -1,
    LineTo = 0,
    BezierTo = 1,
    MoveTo = 2,
}
pub struct PdfPathSegment<'a> {
    point: PathPoint,
    matrix: Option<PdfMatrix>,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPathSegment<'a> {
    pub fn segment_type(&self) -> PdfPathSegmentType {
        match self.point.point_type {
            PointType::Move => PdfPathSegmentType::MoveTo,
            PointType::Line => PdfPathSegmentType::LineTo,
            PointType::Bezier => PdfPathSegmentType::BezierTo,
        }
    }
    pub fn is_close(&self) -> bool {
        self.point.close_figure
    }
    pub fn point(&self) -> (PdfPoints, PdfPoints) {
        let point = (
            PdfPoints::new(self.point.point.x),
            PdfPoints::new(self.point.point.y),
        );
        self.matrix
            .map_or(point, |m| m.apply_to_points(point.0, point.1))
    }
    pub fn x(&self) -> PdfPoints {
        self.point().0
    }
    pub fn y(&self) -> PdfPoints {
        self.point().1
    }
}
/// The zero-based index of a single [PdfPathSegment] inside its containing [PdfPathSegments] collection.
pub type PdfPathSegmentIndex = u32;

/// The collection of [PdfPathSegment] objects inside a path page object, a font glyph path,
/// or a clip path.
pub trait PdfPathSegments<'a> {
    /// Returns the number of path segments in this [PdfPathSegments] collection.
    fn len(&self) -> PdfPathSegmentIndex;

    /// Returns `true` if this [PdfPathSegments] collection is empty.
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a Range from `0..(number of path segments)` for this [PdfPathSegments] collection.
    #[inline]
    fn as_range(&self) -> Range<PdfPathSegmentIndex> {
        0..self.len()
    }

    /// Returns an inclusive Range from `0..=(number of path segments - 1)` for this [PdfPathSegments] collection.
    #[inline]
    fn as_range_inclusive(&self) -> RangeInclusive<PdfPathSegmentIndex> {
        if self.is_empty() {
            0..=0
        } else {
            0..=(self.len() - 1)
        }
    }

    /// Returns a single [PdfPathSegment] from this [PdfPathSegments] collection.
    fn get(&self, index: PdfPathSegmentIndex) -> Result<PdfPathSegment<'a>, PdfiumError>;

    /// Returns an iterator over all the path segments in this [PdfPathSegments] collection.
    fn iter(&'a self) -> PdfPathSegmentsIterator<'a>;
}

/// An iterator over all the [PdfPathSegment] objects in a [PdfPathSegments] collection.
pub struct PdfPathSegmentsIterator<'a> {
    segments: &'a dyn PdfPathSegments<'a>,
    next_index: PdfPathSegmentIndex,
}

impl<'a> PdfPathSegmentsIterator<'a> {
    #[inline]
    pub(crate) fn new(segments: &'a dyn PdfPathSegments<'a>) -> Self {
        PdfPathSegmentsIterator {
            segments,
            next_index: 0,
        }
    }
}

impl<'a> Iterator for PdfPathSegmentsIterator<'a> {
    type Item = PdfPathSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.segments.get(self.next_index);

        self.next_index += 1;

        next.ok()
    }
}
pub struct PdfFontGlyphs<'a> {
    document: Rc<DocumentState>,
    core: CpdfFont,
    len: OnceCell<PdfFontGlyphIndex>,
    lifetime: PhantomData<&'a ()>,
}
pub type PdfFontGlyphIndex = u16;
impl<'a> PdfFontGlyphs<'a> {
    pub(crate) fn from_core(document: Rc<DocumentState>, core: CpdfFont) -> Self {
        Self {
            document,
            core,
            len: OnceCell::new(),
            lifetime: PhantomData,
        }
    }
    fn max_valid(&self, min: u16, max: u16) -> Option<u16> {
        if sdk::glyph_path(&self.core, max.into(), 1.).is_some() {
            return Some(max);
        }
        if sdk::glyph_path(&self.core, min.into(), 1.).is_none() {
            return None;
        }
        let mid = min + (max - min) / 2;
        if sdk::glyph_path(&self.core, mid.into(), 1.).is_none() {
            if mid > min {
                self.max_valid(min, mid - 1)
            } else {
                None
            }
        } else if mid < max {
            self.max_valid(mid + 1, max)
        } else {
            Some(max)
        }
    }
}
impl<'a> PdfFontGlyphs<'a> {
    pub fn len(&self) -> PdfFontGlyphIndex {
        *self
            .len
            .get_or_init(|| self.max_valid(0, u16::MAX).unwrap_or(0))
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfFontGlyphIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfFontGlyphIndex> {
        0..=self.len().saturating_sub(1)
    }
    pub fn get(&self, index: PdfFontGlyphIndex) -> Result<PdfFontGlyph<'a>, PdfiumError> {
        if index >= self.len() {
            return Err(PdfiumError::FontGlyphIndexOutOfBounds);
        }
        Ok(PdfFontGlyph {
            _document: self.document.clone(),
            core: self.core.clone(),
            index,
            lifetime: PhantomData,
        })
    }
    pub fn iter(&self) -> PdfFontGlyphsIterator<'_> {
        PdfFontGlyphsIterator {
            glyphs: self,
            next_index: 0,
        }
    }
}
pub struct PdfFontGlyphsIterator<'a> {
    glyphs: &'a PdfFontGlyphs<'a>,
    next_index: PdfFontGlyphIndex,
}
impl<'a> Iterator for PdfFontGlyphsIterator<'a> {
    type Item = PdfFontGlyph<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.glyphs.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.glyphs.get(i).ok()
    }
}
pub struct PdfFontGlyph<'a> {
    _document: Rc<DocumentState>,
    core: CpdfFont,
    index: PdfFontGlyphIndex,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfFontGlyph<'a> {
    pub fn width_at_font_size(&self, size: PdfPoints) -> PdfPoints {
        PdfPoints::new(sdk::glyph_width(&self.core, self.index.into(), size.value))
    }
    pub fn segments_at_font_size(
        &self,
        size: PdfPoints,
    ) -> Result<PdfFontGlyphPath<'_>, PdfiumError> {
        let path = sdk::glyph_path(&self.core, self.index.into(), size.value).ok_or(
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::Unknown),
        )?;
        Ok(PdfFontGlyphPath {
            path,
            lifetime: PhantomData,
        })
    }
}
pub struct PdfFontGlyphPath<'a> {
    path: Rc<pdfoxium_core::fxge::cfx_path::CfxPath>,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPathSegments<'a> for PdfFontGlyphPath<'a> {
    fn len(&self) -> PdfPathSegmentIndex {
        self.path.points().len() as PdfPathSegmentIndex
    }
    fn get(&self, index: PdfPathSegmentIndex) -> Result<PdfPathSegment<'a>, PdfiumError> {
        let point = *self
            .path
            .points()
            .get(index as usize)
            .ok_or(PdfiumError::FontGlyphIndexOutOfBounds)?;
        Ok(PdfPathSegment {
            point,
            matrix: None,
            lifetime: PhantomData,
        })
    }
    fn iter(&'a self) -> PdfPathSegmentsIterator<'a> {
        PdfPathSegmentsIterator::new(self)
    }
}

impl<'a> PdfPathSegment<'a> {
    pub(crate) fn from_point(point: PathPoint, matrix: Option<PdfMatrix>) -> Self {
        Self {
            point,
            matrix,
            lifetime: PhantomData,
        }
    }
}
