// Interfaces copied from pdfium-render 0.9.4; MIT.
use crate::{object_store::ObjectHandle, prelude::*};
use std::{
    marker::PhantomData,
    ops::{Range, RangeInclusive},
};
pub type PdfClipPathSegmentIndex = u16;
pub struct PdfClipPath<'a> {
    handle: ObjectHandle,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfClipPath<'a> {
    pub(crate) fn from_handle(handle: ObjectHandle) -> Self {
        Self {
            handle,
            lifetime: PhantomData,
        }
    }
}
impl<'a> PdfClipPath<'a> {
    pub fn len(&self) -> PdfClipPathSegmentIndex {
        self.handle
            .read(|o| Ok(o.base().clip_path().path_count().min(u16::MAX as usize) as u16))
            .unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfClipPathSegmentIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfClipPathSegmentIndex> {
        0..=self.len().saturating_sub(1)
    }
    pub fn get(
        &self,
        index: PdfClipPathSegmentIndex,
    ) -> Result<PdfClipPathSegments<'a>, PdfiumError> {
        if index >= self.len() {
            return Err(PdfiumError::PdfClipPathSegmentIndexOutOfBounds);
        }
        Ok(PdfClipPathSegments {
            handle: self.handle.clone(),
            index,
            lifetime: PhantomData,
        })
    }
    pub fn iter(&self) -> PdfClipPathIterator<'_> {
        PdfClipPathIterator {
            clip_path: self,
            next_index: 0,
        }
    }
}
pub struct PdfClipPathIterator<'a> {
    clip_path: &'a PdfClipPath<'a>,
    next_index: PdfClipPathSegmentIndex,
}
impl<'a> Iterator for PdfClipPathIterator<'a> {
    type Item = PdfClipPathSegments<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.clip_path.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.clip_path.get(i).ok()
    }
}
pub struct PdfClipPathSegments<'a> {
    handle: ObjectHandle,
    index: PdfClipPathSegmentIndex,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPathSegments<'a> for PdfClipPathSegments<'a> {
    fn len(&self) -> PdfPathSegmentIndex {
        self.handle
            .read(|o| {
                let clip = o.base().clip_path();
                if !clip.has_ref() || self.index as usize >= clip.path_count() {
                    return Err(PdfiumError::PdfClipPathSegmentIndexOutOfBounds);
                }
                Ok(clip
                    .path(self.index as usize)
                    .get_object()
                    .map_or(0, |p| p.points().len() as u32))
            })
            .unwrap_or(0)
    }
    fn get(&self, index: PdfPathSegmentIndex) -> Result<PdfPathSegment<'a>, PdfiumError> {
        self.handle.read(|o| {
            let clip = o.base().clip_path();
            if !clip.has_ref() || self.index as usize >= clip.path_count() {
                return Err(PdfiumError::PdfClipPathSegmentIndexOutOfBounds);
            }
            let point = clip
                .path(self.index as usize)
                .get_object()
                .and_then(|p| p.points().get(index as usize))
                .copied()
                .ok_or(PdfiumError::PdfClipPathSegmentIndexOutOfBounds)?;
            Ok(PdfPathSegment::from_point(point, None))
        })
    }
    fn iter(&'a self) -> PdfPathSegmentsIterator<'a> {
        PdfPathSegmentsIterator::new(self)
    }
}
