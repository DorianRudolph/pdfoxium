// Interfaces copied from pdfium-render 0.9.4; MIT.
use crate::{backend, create_transform_getters, object_store::ObjectHandle, prelude::*};
use pdfoxium_core::{
    fpdftext::{
        cpdf_textpage::{CharType, CpdfTextPage},
        cpdf_textpagefind::{CpdfTextPageFind, Options},
    },
    fxcrt::fx_coordinates::{CfxPointF, CfxSizeF},
};
use std::{
    cell::RefCell,
    fmt::{Display, Formatter},
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};
pub type PdfPageTextCharIndex = usize;
pub type PdfPageTextSegmentIndex = usize;
#[derive(Clone)]
pub struct PdfPageText<'a> {
    pub(crate) core: Rc<CpdfTextPage>,
    page: backend::PdfPage,
    epoch: u64,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPageText<'a> {
    pub(crate) fn from_page(page: backend::PdfPage) -> Result<Self, PdfiumError> {
        page.check_live()?;
        let core = backend::text::PdfPageText::new(&page)?.core;
        let epoch = page.state.epoch.get();
        Ok(Self {
            core,
            page,
            epoch,
            lifetime: PhantomData,
        })
    }
    fn near(
        &self,
        x: PdfPoints,
        tolerance_x: PdfPoints,
        y: PdfPoints,
        tolerance_y: PdfPoints,
    ) -> Option<usize> {
        usize::try_from(self.core.get_index_at_pos(
            CfxPointF::new(x.value, y.value),
            CfxSizeF::new(tolerance_x.value, tolerance_y.value),
        ))
        .ok()
    }
    fn subset(&self, indices: Vec<usize>) -> PdfPageTextChars<'a> {
        PdfPageTextChars {
            text: self.clone(),
            indices,
        }
    }
    fn char_at(&self, index: usize) -> PdfPageTextChar<'a> {
        PdfPageTextChar {
            text: self.clone(),
            index,
        }
    }
    fn segments_for(&self, start: usize, count: usize) -> PdfPageTextSegments<'a> {
        PdfPageTextSegments {
            text: self.clone(),
            rects: self.core.get_rect_array(
                start.min(i32::MAX as usize) as i32,
                count.min(i32::MAX as usize) as i32,
            ),
        }
    }
}
impl<'a> PdfPageText<'a> {
    pub fn len(&self) -> i32 {
        self.core.count_chars()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn segments(&self) -> PdfPageTextSegments<'_> {
        self.segments_for(0, self.len() as usize)
    }
    pub fn segments_subset(
        &self,
        start: PdfPageTextCharIndex,
        count: PdfPageTextCharIndex,
    ) -> PdfPageTextSegments<'_> {
        self.segments_for(start, count)
    }
    pub fn chars(&self) -> PdfPageTextChars<'_> {
        self.subset((0..self.core.size()).collect())
    }
    pub fn chars_for_object(
        &self,
        object: &PdfPageTextObject,
    ) -> Result<PdfPageTextChars<'_>, PdfiumError> {
        let id = object.handle.core_id();
        Ok(self.subset(
            (0..self.core.size())
                .filter(|&i| self.core.get_char_info(i).text_object() == id && id.is_some())
                .collect(),
        ))
    }
    pub fn chars_for_annotation(
        &self,
        annotation: &PdfPageAnnotation,
    ) -> Result<PdfPageTextChars<'_>, PdfiumError> {
        self.chars_inside_rect(annotation.bounds()?)
            .map_err(|_| PdfiumError::NoCharsInAnnotation)
    }
    pub fn chars_inside_rect<'b>(
        &'b self,
        rect: PdfRect,
    ) -> Result<PdfPageTextChars<'a>, PdfiumError> {
        let tx = rect.width() / 2.;
        let ty = rect.height() / 2.;
        let y = rect.bottom() + ty;
        let start = self.near(rect.left(), tx, y, ty);
        let end = self.near(rect.right(), tx, y, ty);
        let (start, end) = match (start, end) {
            (Some(a), Some(b)) => (a, b.saturating_add(1)),
            (Some(a), None) | (None, Some(a)) => (a, a.saturating_add(1)),
            _ => return Err(PdfiumError::NoCharsInRect),
        };
        Ok(self.subset((start..=end.min(self.core.size().saturating_sub(1))).collect()))
    }
    pub fn all(&self) -> String {
        self.inside_rect(self.page.page_size())
    }
    pub fn inside_rect(&self, rect: PdfRect) -> String {
        backend::text::decode(&self.core.get_text_by_rect(rect.as_core()))
    }
    pub fn for_object(&self, object: &PdfPageTextObject) -> String {
        object
            .handle
            .core_id()
            .map(|id| backend::text::decode(&self.core.get_text_by_object_id(id)))
            .unwrap_or_default()
    }
    pub fn for_annotation(&self, annotation: &PdfPageAnnotation) -> Result<String, PdfiumError> {
        let bounds = annotation.bounds()?;

        Ok(self.inside_rect(bounds))
    }
    pub fn search(
        &self,
        text: &str,
        options: &PdfSearchOptions,
    ) -> Result<PdfPageTextSearch<'_>, PdfiumError> {
        self.search_from(text, options, 0)
    }
    pub fn search_from(
        &self,
        text: &str,
        options: &PdfSearchOptions,
        index: PdfPageTextCharIndex,
    ) -> Result<PdfPageTextSearch<'_>, PdfiumError> {
        if text.is_empty() {
            return Err(PdfiumError::TextSearchTargetIsEmpty);
        }
        Ok(PdfPageTextSearch {
            text: self.clone(),
            finder: RefCell::new(CpdfTextPageFind::new(
                self.core.clone(),
                &text.chars().map(u32::from).collect::<Vec<_>>(),
                Options {
                    match_case: options.match_case,
                    match_whole_word: options.match_whole_word,
                    ..Options::default()
                },
                Some(index),
            )),
        })
    }
}
impl Display for PdfPageText<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.all())
    }
}
pub struct PdfPageTextChars<'a> {
    text: PdfPageText<'a>,
    indices: Vec<usize>,
}
impl<'a> PdfPageTextChars<'a> {
    pub fn first_char_index(&self) -> Option<PdfPageTextCharIndex> {
        self.indices.first().copied()
    }
    pub fn len(&self) -> PdfPageTextCharIndex {
        self.indices.len()
    }
    pub fn last_char_index(&self) -> Option<PdfPageTextCharIndex> {
        self.indices.last().copied()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get<'b>(
        &'b self,
        index: PdfPageTextCharIndex,
    ) -> Result<PdfPageTextChar<'a>, PdfiumError> {
        let i = *self
            .indices
            .get(index)
            .ok_or(PdfiumError::CharIndexOutOfBounds)?;
        Ok(self.text.char_at(i))
    }
    pub fn first(&self) -> Result<PdfPageTextChar<'a>, PdfiumError> {
        if !self.is_empty() {
            self.get(0)
        } else {
            Err(PdfiumError::NoCharsInPageTextChars)
        }
    }
    pub fn last(&self) -> Result<PdfPageTextChar<'a>, PdfiumError> {
        if !self.is_empty() {
            self.get(self.len() - 1)
        } else {
            Err(PdfiumError::NoCharsInPageTextChars)
        }
    }
    pub fn get_char_at_point(&self, x: PdfPoints, y: PdfPoints) -> Option<PdfPageTextChar<'_>> {
        self.get_char_near_point(x, PdfPoints::ZERO, y, PdfPoints::ZERO)
    }
    pub fn get_char_near_point(
        &self,
        x: PdfPoints,
        tolerance_x: PdfPoints,
        y: PdfPoints,
        tolerance_y: PdfPoints,
    ) -> Option<PdfPageTextChar<'_>> {
        let index = self.text.near(x, tolerance_x, y, tolerance_y)?;
        self.get(index).ok()
    }
    pub fn iter(&self) -> PdfPageTextCharsIterator<'_> {
        PdfPageTextCharsIterator {
            chars: self,
            next_index: 0,
        }
    }
}
pub struct PdfPageTextCharsIterator<'a> {
    chars: &'a PdfPageTextChars<'a>,
    next_index: usize,
}
impl<'a> Iterator for PdfPageTextCharsIterator<'a> {
    type Item = PdfPageTextChar<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.chars.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.chars.get(i).ok()
    }
}
pub struct PdfPageTextChar<'a> {
    text: PdfPageText<'a>,
    index: usize,
}
impl<'a> PdfPageTextChar<'a> {
    fn font(&self) -> Option<PdfFont<'a>> {
        self.text
            .core
            .get_char_font(self.index)
            .cloned()
            .map(|f| PdfFont::from_existing(self.text.page.document.clone(), f))
    }
    create_transform_getters!("this character", "this character.", "this character,");
    fn get_matrix_impl(&self) -> Result<PdfMatrix, PdfiumError> {
        let m = self.text.core.get_char_info(self.index).matrix();
        Ok(PdfMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f))
    }
}
impl<'a> PdfPageTextChar<'a> {
    pub fn index(&self) -> PdfPageTextCharIndex {
        self.index
    }
    pub fn unicode_value(&self) -> u32 {
        self.text.core.get_char_info(self.index).unicode()
    }
    pub fn unicode_char(&self) -> Option<char> {
        char::from_u32(self.unicode_value())
    }
    pub fn unicode_string(&self) -> Option<String> {
        self.unicode_char().map(|char| char.to_string())
    }
    pub fn scaled_font_size(&self) -> PdfPoints {
        PdfPoints::new(self.unscaled_font_size().value * self.get_vertical_scale())
    }
    pub fn unscaled_font_size(&self) -> PdfPoints {
        PdfPoints::new(self.text.core.get_char_font_size(self.index))
    }
    pub fn font_name(&self) -> String {
        self.font().map(|f| f.name()).unwrap_or_default()
    }
    pub fn font_weight(&self) -> Option<PdfFontWeight> {
        self.font().and_then(|f| f.weight().ok())
    }
    pub fn font_is_fixed_pitch(&self) -> bool {
        self.font().is_some_and(|f| f.is_fixed_pitch())
    }
    pub fn font_is_proportional_pitch(&self) -> bool {
        !self.font_is_fixed_pitch()
    }
    pub fn font_is_serif(&self) -> bool {
        self.font().is_some_and(|f| f.is_serif())
    }
    pub fn font_is_sans_serif(&self) -> bool {
        !self.font_is_serif()
    }
    pub fn font_is_symbolic(&self) -> bool {
        self.font().is_some_and(|f| f.is_symbolic())
    }
    pub fn font_is_non_symbolic(&self) -> bool {
        // This flag bit and the symbolic flag bit cannot both be set or both be clear.

        self.font().is_some_and(|f| f.is_non_symbolic())
    }
    pub fn font_is_cursive(&self) -> bool {
        self.font().is_some_and(|f| f.is_cursive())
    }
    pub fn font_is_italic(&self) -> bool {
        self.font().is_some_and(|f| f.is_italic())
    }
    pub fn font_is_all_caps(&self) -> bool {
        self.font().is_some_and(|f| f.is_all_caps())
    }
    pub fn font_is_small_caps(&self) -> bool {
        self.font().is_some_and(|f| f.is_small_caps())
    }
    pub fn font_is_bold_reenforced(&self) -> bool {
        self.font().is_some_and(|f| f.is_bold_reenforced())
    }
    pub fn text_object(&self) -> Result<PdfPageTextObject<'_>, PdfiumError> {
        if self.text.epoch != self.text.page.state.epoch.get() {
            return Err(PdfiumError::OwnershipNotAttachedToPage);
        }
        let id = self
            .text
            .core
            .get_char_info(self.index)
            .text_object()
            .ok_or(PdfiumError::NoCharsInPageObject)?;
        fn find<'a>(
            handle: ObjectHandle,
            id: pdfoxium_core::fpdfapi::page::cpdf_pageobjectholder::PageObjectId,
        ) -> Option<PdfPageTextObject<'a>> {
            if handle.core_id() == Some(id) {
                return PdfPageObject::from_handle(handle)
                    .ok()
                    .and_then(|o| match o {
                        PdfPageObject::Text(t) => Some(t),
                        _ => None,
                    });
            }
            let count=handle.read(|o|Ok(o.as_any().downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_formobject::CpdfFormObject>().map_or(0,|f|f.form().holder().page_object_count()))).ok()?;
            for i in 0..count {
                if let Some(object) = ObjectHandle::form(handle.clone(), i)
                    .ok()
                    .and_then(|h| find(h, id))
                {
                    return Some(object);
                }
            }
            None
        }
        let count = self
            .text
            .page
            .state
            .core
            .borrow()
            .holder()
            .page_object_count();
        for i in 0..count {
            if let Some(object) = ObjectHandle::page(&self.text.page, i)
                .ok()
                .and_then(|h| find(h, id))
            {
                return Ok(object);
            }
        }
        Err(PdfiumError::NoCharsInPageObject)
    }
    pub fn render_mode(&self) -> Result<PdfPageTextRenderMode, PdfiumError> {
        self.text_object().map(|o| o.render_mode())
    }
    pub fn fill_color(&self) -> Result<PdfColor, PdfiumError> {
        let info = self.text.core.get_char_info(self.index);
        let info = info
            .text_object()
            .and_then(|id| self.text.core.text_object_info(id))
            .ok_or(PdfiumError::NoCharsInPageObject)?;
        let c = info.fill_color;
        Ok(PdfColor::new(
            c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8,
        ))
    }
    pub fn stroke_color(&self) -> Result<PdfColor, PdfiumError> {
        let info = self.text.core.get_char_info(self.index);
        let info = info
            .text_object()
            .and_then(|id| self.text.core.text_object_info(id))
            .ok_or(PdfiumError::NoCharsInPageObject)?;
        let c = info.stroke_color;
        Ok(PdfColor::new(
            c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8,
        ))
    }
    pub fn angle_degrees(&self) -> Result<f32, PdfiumError> {
        self.angle_radians().map(|result| result.to_degrees())
    }
    pub fn angle_radians(&self) -> Result<f32, PdfiumError> {
        let m = self.text.core.get_char_info(self.index).matrix();
        let angle = m.b.atan2(m.a);
        Ok(if angle < 0. {
            angle + std::f32::consts::TAU
        } else {
            angle
        })
    }
    pub fn tight_bounds(&self) -> Result<PdfRect, PdfiumError> {
        Ok(PdfRect::from_core(
            self.text.core.get_char_info(self.index).char_box(),
        ))
    }
    pub fn loose_bounds(&self) -> Result<PdfRect, PdfiumError> {
        Ok(PdfRect::from_core(
            self.text.core.get_char_loose_bounds(self.index),
        ))
    }
    pub fn origin(&self) -> Result<(PdfPoints, PdfPoints), PdfiumError> {
        let p = self.text.core.get_char_info(self.index).origin();
        Ok((PdfPoints::new(p.x), PdfPoints::new(p.y)))
    }
    pub fn origin_x(&self) -> Result<PdfPoints, PdfiumError> {
        self.origin().map(|result| result.0)
    }
    pub fn origin_y(&self) -> Result<PdfPoints, PdfiumError> {
        self.origin().map(|result| result.1)
    }
    pub fn has_descender(&self) -> bool {
        self.tight_bounds()
            .map(|bounds| bounds.bottom().value)
            .unwrap_or(0.0)
            < self
                .loose_bounds()
                .map(|bounds| bounds.bottom().value)
                .unwrap_or(0.0)
    }
    pub fn is_generated(&self) -> Result<bool, PdfiumError> {
        Ok(self.text.core.get_char_info(self.index).char_type() == CharType::Generated)
    }
    pub fn is_hyphen(&self) -> Result<bool, PdfiumError> {
        Ok(self.text.core.get_char_info(self.index).char_type() == CharType::Hyphen)
    }
}
pub struct PdfPageTextSegments<'a> {
    text: PdfPageText<'a>,
    rects: Vec<pdfoxium_core::fxcrt::fx_coordinates::CfxFloatRect>,
}
impl<'a> PdfPageTextSegments<'a> {
    pub fn len(&self) -> PdfPageTextSegmentIndex {
        self.rects.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfPageTextSegmentIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfPageTextSegmentIndex> {
        if self.is_empty() {
            0..=0
        } else {
            0..=(self.len() - 1)
        }
    }
    pub fn get<'b>(
        &'b self,
        index: PdfPageTextSegmentIndex,
    ) -> Result<PdfPageTextSegment<'a>, PdfiumError> {
        let rect = *self
            .rects
            .get(index)
            .ok_or(PdfiumError::TextSegmentIndexOutOfBounds)?;
        Ok(PdfPageTextSegment {
            text: self.text.clone(),
            rect: PdfRect::from_core(rect),
        })
    }
    pub fn first(&self) -> Result<PdfPageTextSegment<'a>, PdfiumError> {
        if !self.is_empty() {
            self.get(0)
        } else {
            Err(PdfiumError::NoTextSegmentsInPageText)
        }
    }
    pub fn last(&self) -> Result<PdfPageTextSegment<'a>, PdfiumError> {
        if !self.is_empty() {
            self.get(self.len() - 1)
        } else {
            Err(PdfiumError::NoTextSegmentsInPageText)
        }
    }
    pub fn iter(&self) -> PdfPageTextSegmentsIterator<'_> {
        PdfPageTextSegmentsIterator {
            segments: self,
            next_index: 0,
        }
    }
}
pub struct PdfPageTextSegmentsIterator<'a> {
    segments: &'a PdfPageTextSegments<'a>,
    next_index: usize,
}
impl<'a> Iterator for PdfPageTextSegmentsIterator<'a> {
    type Item = PdfPageTextSegment<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.segments.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.segments.get(i).ok()
    }
}
pub struct PdfPageTextSegment<'a> {
    text: PdfPageText<'a>,
    rect: PdfRect,
}
impl<'a> PdfPageTextSegment<'a> {
    pub fn bounds(&self) -> PdfRect {
        self.rect
    }
    pub fn width(&self) -> PdfPoints {
        self.bounds().width()
    }
    pub fn height(&self) -> PdfPoints {
        self.bounds().height()
    }
    pub fn is_inside_rect(&self, rect: &PdfRect) -> bool {
        self.bounds().is_inside(rect)
    }
    pub fn does_overlap_rect(&self, rect: &PdfRect) -> bool {
        self.bounds().does_overlap(rect)
    }
    pub fn text(&self) -> String {
        self.text.inside_rect(self.rect)
    }
    pub fn chars<'b>(&'b self) -> Result<PdfPageTextChars<'a>, PdfiumError> {
        self.text.chars_inside_rect(self.rect)
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PdfSearchOptions {
    match_case: bool,
    match_whole_word: bool,
}
impl PdfSearchOptions {
    pub fn new() -> Self {
        PdfSearchOptions {
            match_case: false,
            match_whole_word: false,
        }
    }
    pub fn match_case(mut self, do_match_case: bool) -> Self {
        self.match_case = do_match_case;

        self
    }
    pub fn match_whole_word(mut self, do_match_whole_word: bool) -> Self {
        self.match_whole_word = do_match_whole_word;

        self
    }
}
impl Default for PdfSearchOptions {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum PdfSearchDirection {
    SearchForward,
    SearchBackward,
}
pub struct PdfPageTextSearch<'a> {
    text: PdfPageText<'a>,
    finder: RefCell<CpdfTextPageFind>,
}
impl<'a> PdfPageTextSearch<'a> {
    pub fn find_next(&self) -> Option<PdfPageTextSegments<'_>> {
        self.get_next_result(PdfSearchDirection::SearchForward)
    }
    pub fn find_previous(&self) -> Option<PdfPageTextSegments<'_>> {
        self.get_next_result(PdfSearchDirection::SearchBackward)
    }
    pub fn get_next_result(
        &self,
        direction: PdfSearchDirection,
    ) -> Option<PdfPageTextSegments<'_>> {
        let mut f = self.finder.borrow_mut();
        let found = match direction {
            PdfSearchDirection::SearchForward => f.find_next(),
            PdfSearchDirection::SearchBackward => f.find_prev(),
        };
        if !found {
            return None;
        }
        Some(
            self.text
                .segments_for(f.get_cur_order() as usize, f.get_matched_count() as usize),
        )
    }
    pub fn iter(&self, direction: PdfSearchDirection) -> PdfPageTextSearchIterator<'_> {
        PdfPageTextSearchIterator {
            search: self,
            direction,
        }
    }
}
pub struct PdfPageTextSearchIterator<'a> {
    search: &'a PdfPageTextSearch<'a>,
    direction: PdfSearchDirection,
}
impl<'a> Iterator for PdfPageTextSearchIterator<'a> {
    type Item = PdfPageTextSegments<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.search.get_next_result(self.direction)
    }
}
