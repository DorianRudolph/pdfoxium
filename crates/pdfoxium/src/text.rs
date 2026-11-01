// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! PDFium text extraction and matching with UTF-8 offsets and optional rectangles.
//!
//! Offsets refer to the exact extracted string, not visual reading order or
//! grapheme clusters. The snapshot retains character geometry; ordinary search
//! does not construct highlight rectangles. Malformed Unicode is replaced with
//! U+FFFD, and retained UTF-16 surrogate pairs are combined during conversion.

use crate::{Document, Error, Page, PageId, Point, Rect, Result, Size};
use pdfoxium_core::{
    fpdfdoc::cpdf_viewerpreferences::CpdfViewerPreferences,
    fpdftext::{
        cpdf_textpage::CpdfTextPage,
        cpdf_textpagefind::{CpdfTextPageFind, Options},
    },
    fxcrt::fx_coordinates::{CfxPointF, CfxSizeF},
};
use std::{ops::Range, rc::Rc};

mod selection;

/// PDFium matching rules (including its whitespace and whole-word behavior),
/// rather than regular expressions or full Unicode case folding.
#[derive(Clone, Copy, Debug, Default)]
pub struct SearchOptions {
    pub match_case: bool,
    pub match_whole_word: bool,
    /// Permit the next match to overlap the previous one.
    pub overlapping: bool,
}

struct Snapshot {
    core: Rc<CpdfTextPage>,
    text: String,
    // One UTF-8 span per core text unit; paired surrogate units share a span.
    units: Vec<Range<usize>>,
    page_id: PageId,
}

/// Immutable, owned text snapshot. Cloning is cheap. It survives page unload and
/// document drop, but retains core font metadata and stays on its owning thread.
#[derive(Clone)]
pub struct TextPage(Rc<Snapshot>);

impl Page {
    pub fn text(&self) -> Result<TextPage> {
        let lease = self.acquire_render()?;
        let _call = lease.enter_core()?;
        let rtl = CpdfViewerPreferences::new(&lease.core_document().borrow()).is_direction_r2l();
        let page = lease.pdf_page()?;
        let core = Rc::new(CpdfTextPage::new(&page, rtl));
        lease.check_io()?;
        Ok(TextPage::from_core(core, self.id()))
    }
}

fn encode_units(units: &[u32]) -> (String, Vec<Range<usize>>) {
    let mut text = String::new();
    let mut spans = Vec::with_capacity(units.len());
    let mut index = 0;
    while index < units.len() {
        let first = units[index];
        let paired = (0xd800..=0xdbff).contains(&first)
            && units
                .get(index + 1)
                .is_some_and(|low| (0xdc00..=0xdfff).contains(low));
        let scalar = if paired {
            0x10000 + ((first - 0xd800) << 10) + units[index + 1] - 0xdc00
        } else {
            first
        };
        let start = text.len();
        text.push(char::from_u32(scalar).unwrap_or(char::REPLACEMENT_CHARACTER));
        let span = start..text.len();
        spans.push(span.clone());
        if paired {
            spans.push(span);
        }
        index += if paired { 2 } else { 1 };
    }
    (text, spans)
}

impl TextPage {
    fn from_core(core: Rc<CpdfTextPage>, page_id: PageId) -> Self {
        let (text, units) = encode_units(&core.get_all_page_text());
        Self(Rc::new(Snapshot {
            core,
            text,
            units,
            page_id,
        }))
    }

    pub fn as_str(&self) -> &str {
        &self.0.text
    }
    pub fn page_id(&self) -> PageId {
        self.0.page_id
    }

    /// Starts a forward search. Empty queries are rejected. Hits refer to this
    /// snapshot, and preserve PDFium's character-range matching semantics.
    pub fn search(&self, query: &str, options: SearchOptions) -> Result<Search> {
        if query.is_empty() {
            return Err(Error::InvalidArgument("search query must not be empty"));
        }
        let query: Vec<u32> = query.chars().map(u32::from).collect();
        let finder = CpdfTextPageFind::new(
            self.0.core.clone(),
            &query,
            Options {
                match_case: options.match_case,
                match_whole_word: options.match_whole_word,
                consecutive: options.overlapping,
            },
            Some(0),
        );
        Ok(Search {
            text: self.clone(),
            finder,
            done: false,
        })
    }

    /// Same matches as `search`, constructing rectangles only for each yielded hit.
    pub fn search_with_rects(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<SearchWithRects> {
        Ok(SearchWithRects {
            search: self.search(query, options)?,
        })
    }

    fn bytes_for_chars(&self, chars: Range<usize>) -> Result<Range<usize>> {
        // Only the endpoints matter. Do not translate every character in a
        // long match: each core lookup scans its compact segment table.
        let unit_at = |index: usize| {
            let index = self.0.core.text_index_from_char_index(index as i32);
            (index >= 0).then_some(index as usize)
        };
        let first = chars.clone().find_map(unit_at);
        let last = chars.rev().find_map(unit_at);
        match (first, last) {
            (Some(first), Some(last)) => {
                let start = self.0.units.get(first).ok_or(Error::Internal(
                    "text index outside extracted UTF-8 mapping",
                ))?;
                let end = self.0.units.get(last).ok_or(Error::Internal(
                    "text index outside extracted UTF-8 mapping",
                ))?;
                Ok(start.start..end.end)
            }
            _ => Err(Error::InvalidArgument(
                "selected characters have no extracted text",
            )),
        }
    }

    fn rects_for_chars(&self, chars: Range<usize>) -> Vec<Rect> {
        self.0
            .core
            .get_rect_array(chars.start as i32, chars.len() as i32)
            .into_iter()
            .filter(|r| r.width() > 0.0 && r.height() > 0.0)
            .map(|r| Rect {
                left: r.left,
                bottom: r.bottom,
                right: r.right,
                top: r.top,
            })
            .collect()
    }

    /// Select an exact UTF-8 slice. Highlighting may cover a whole glyph when
    /// multiple text elements share geometry. Empty ranges are valid.
    pub fn select(&self, range: Range<usize>) -> Result<TextSelection> {
        if self.as_str().get(range.clone()).is_none() {
            return Err(Error::InvalidArgument(
                "text range must have valid UTF-8 boundaries",
            ));
        }
        let chars = if range.is_empty() {
            0..0
        } else {
            let first = self.0.units.partition_point(|span| span.end <= range.start);
            let end = self.0.units.partition_point(|span| span.start < range.end);
            let start = self.0.core.char_index_from_text_index(first as i32);
            let last = self.0.core.char_index_from_text_index((end - 1) as i32);
            if start < 0 || last < start {
                return Err(Error::Internal("missing character mapping"));
            }
            start as usize..last as usize + 1
        };
        Ok(TextSelection {
            text: self.clone(),
            byte_range: range,
            chars,
        })
    }

    /// PDFium character hit testing, in unscaled page coordinates. Tolerance
    /// is the total added box width/height, extending half on either side.
    pub fn hit_test(&self, point: Point, tolerance: Size) -> Result<Option<TextPosition>> {
        if !point.x.is_finite()
            || !point.y.is_finite()
            || !tolerance.width.is_finite()
            || !tolerance.height.is_finite()
            || tolerance.width < 0.0
            || tolerance.height < 0.0
        {
            return Err(Error::InvalidArgument(
                "hit-test coordinates and tolerances must be finite and tolerances nonnegative",
            ));
        }
        let index = self.0.core.get_index_at_pos(
            CfxPointF::new(point.x, point.y),
            CfxSizeF::new(tolerance.width, tolerance.height),
        );
        Ok((index >= 0).then(|| TextPosition {
            text: self.clone(),
            index: index as usize,
        }))
    }

    /// Includes both hit-tested endpoint characters, irrespective of drag direction.
    /// Positions from another extraction snapshot are rejected, even on the same page.
    pub fn select_between(
        &self,
        anchor: &TextPosition,
        focus: &TextPosition,
    ) -> Result<TextSelection> {
        if !Rc::ptr_eq(&self.0, &anchor.text.0) || !Rc::ptr_eq(&self.0, &focus.text.0) {
            return Err(Error::InvalidArgument(
                "selection positions belong to a different text snapshot",
            ));
        }
        let chars = anchor.index.min(focus.index)..anchor.index.max(focus.index) + 1;
        Ok(TextSelection {
            text: self.clone(),
            byte_range: self.bytes_for_chars(chars.clone())?,
            chars,
        })
    }
}

#[derive(Clone)]
pub struct TextPosition {
    text: TextPage,
    index: usize,
}

#[derive(Clone)]
pub struct TextSelection {
    text: TextPage,
    byte_range: Range<usize>,
    chars: Range<usize>,
}
impl TextSelection {
    pub fn text(&self) -> &str {
        &self.text.as_str()[self.byte_range.clone()]
    }
    pub fn byte_range(&self) -> Range<usize> {
        self.byte_range.clone()
    }
    pub fn rects(&self) -> Vec<Rect> {
        self.text.rects_for_chars(self.chars.clone())
    }
    /// Loose font-metric bounds merged into adjacent horizontal text runs,
    /// suitable for a continuous on-screen selection overlay. Text, byte ranges
    /// and the tight PDFium rectangles returned by `rects()` are unchanged.
    pub fn display_rects(&self) -> Vec<Rect> {
        selection::display_rects(self)
    }
}

pub struct Search {
    text: TextPage,
    finder: CpdfTextPageFind,
    done: bool,
}
impl Iterator for Search {
    type Item = Result<SearchHit>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done || !self.finder.find_next() {
            self.done = true;
            return None;
        }
        let start = self.finder.get_cur_order();
        let count = self.finder.get_matched_count();
        if start < 0 || count <= 0 {
            self.done = true;
            return Some(Err(Error::Internal(
                "search returned an invalid character range",
            )));
        }
        let chars = start as usize..start as usize + count as usize;
        Some(
            self.text
                .bytes_for_chars(chars.clone())
                .map(|byte_range| SearchHit {
                    text: self.text.clone(),
                    byte_range,
                    chars,
                }),
        )
    }
}

pub struct SearchHit {
    text: TextPage,
    byte_range: Range<usize>,
    chars: Range<usize>,
}
impl SearchHit {
    pub fn text(&self) -> &str {
        &self.text.as_str()[self.byte_range.clone()]
    }
    pub fn byte_range(&self) -> Range<usize> {
        self.byte_range.clone()
    }
    pub fn page_id(&self) -> PageId {
        self.text.page_id()
    }
}

pub struct SearchWithRects {
    search: Search,
}
impl Iterator for SearchWithRects {
    type Item = Result<SearchHitWithRects>;
    fn next(&mut self) -> Option<Self::Item> {
        self.search.next().map(|hit| {
            hit.map(|hit| {
                let rects = hit.text.rects_for_chars(hit.chars.clone());
                SearchHitWithRects { hit, rects }
            })
        })
    }
}
pub struct SearchHitWithRects {
    pub hit: SearchHit,
    pub rects: Vec<Rect>,
}
impl SearchHitWithRects {
    pub fn text(&self) -> &str {
        self.hit.text()
    }
    pub fn byte_range(&self) -> Range<usize> {
        self.hit.byte_range()
    }
    pub fn page_id(&self) -> PageId {
        self.hit.page_id()
    }
}

/// A detached document-wide result; keeping it does not retain a text page.
#[derive(Clone, Debug)]
pub struct DocumentSearchHit {
    pub page_id: PageId,
    pub page_index: usize,
    pub text: String,
    pub byte_range: Range<usize>,
}
#[derive(Clone, Debug)]
pub struct DocumentSearchHitWithRects {
    pub hit: DocumentSearchHit,
    pub rects: Vec<Rect>,
}

pub struct DocumentSearch {
    document: Document,
    query: String,
    options: SearchOptions,
    page_count: usize,
    revision: u64,
    next_page: usize,
    current: Option<(PageId, usize, Search)>,
    done: bool,
}
impl Document {
    /// Search one page at a time. Captures page order at iterator creation and
    /// returns owned hits. Pages loaded only for this search are unloaded after
    /// extraction; already-resident pages are preserved.
    pub fn search(&self, query: &str, options: SearchOptions) -> Result<DocumentSearch> {
        if query.is_empty() {
            return Err(Error::InvalidArgument("search query must not be empty"));
        }
        let page_count = self.page_count()?;
        Ok(DocumentSearch {
            document: self.clone(),
            query: query.into(),
            options,
            page_count,
            revision: self.revision(),
            next_page: 0,
            current: None,
            done: false,
        })
    }
    pub fn search_with_rects(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<DocumentSearchWithRects> {
        Ok(DocumentSearchWithRects {
            search: self.search(query, options)?,
        })
    }
}
impl DocumentSearch {
    fn next_match(&mut self) -> Option<Result<(usize, SearchHit)>> {
        if self.done {
            return None;
        }
        if self.document.revision() != self.revision {
            self.done = true;
            self.current = None;
            return Some(Err(Error::Stale));
        }
        loop {
            if let Some((_, index, search)) = &mut self.current {
                if let Some(hit) = search.next() {
                    return Some(hit.map(|hit| (*index, hit)));
                }
            }
            self.current = None;
            if self.next_page >= self.page_count {
                self.done = true;
                return None;
            }
            let index = self.next_page;
            self.next_page += 1;
            let prepared = (|| -> Result<_> {
                let page = self.document.page_handle(index)?;
                let id = page.id();
                let loaded = page.is_loaded()?;
                let text = page.text();
                // Release newly loaded content even when extraction reports an error.
                let unload = if loaded { Ok(()) } else { page.unload() };
                let text = text?;
                unload?;
                Ok((id, page.index(), text.search(&self.query, self.options)?))
            })();
            match prepared {
                Ok(current) => self.current = Some(current),
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            }
        }
    }
}
fn detached(index: usize, hit: &SearchHit) -> DocumentSearchHit {
    DocumentSearchHit {
        page_id: hit.page_id(),
        page_index: index,
        text: hit.text().to_owned(),
        byte_range: hit.byte_range(),
    }
}
impl Iterator for DocumentSearch {
    type Item = Result<DocumentSearchHit>;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_match()
            .map(|result| result.map(|(index, hit)| detached(index, &hit)))
    }
}
pub struct DocumentSearchWithRects {
    search: DocumentSearch,
}
impl Iterator for DocumentSearchWithRects {
    type Item = Result<DocumentSearchHitWithRects>;
    fn next(&mut self) -> Option<Self::Item> {
        self.search.next_match().map(|result| {
            result.map(|(index, hit)| DocumentSearchHitWithRects {
                rects: hit.text.rects_for_chars(hit.chars.clone()),
                hit: detached(index, &hit),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::encode_units;
    #[test]
    fn utf8_spans_combine_surrogates_and_replace_invalid_units() {
        let (text, spans) = encode_units(&[65, 0xe9, 0xd83d, 0xde42, 0x1f642, 0xd800, 0x110000]);
        assert_eq!(text, "Aé🙂🙂��");
        assert_eq!(spans, [0..1, 1..3, 3..7, 3..7, 7..11, 11..14, 14..17]);
        assert!(spans.into_iter().all(|span| text.get(span).is_some()));
    }
    #[test]
    fn document_search_only_unloads_its_own_residency() {
        use crate::{Document, SearchOptions};
        let document = Document::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/render_corpus/pdfs/hello_world.pdf"
        )))
        .unwrap();
        let page = document.page_handle(0).unwrap();
        assert!(!page.is_loaded().unwrap());
        let mut search = document.search("Hello", SearchOptions::default()).unwrap();
        assert!(!page.is_loaded().unwrap());
        assert!(search.next().unwrap().is_ok());
        assert!(!page.is_loaded().unwrap());
        drop(search);
        let page = document.page(0).unwrap();
        assert!(page.is_loaded().unwrap());
        let mut search = document.search("Hello", SearchOptions::default()).unwrap();
        assert!(search.next().unwrap().is_ok());
        drop(search);
        assert!(page.is_loaded().unwrap());
    }
}
