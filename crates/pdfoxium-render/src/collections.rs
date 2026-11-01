// Interfaces copied from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::backend;
use crate::prelude::*;
use std::{
    io::{Read, Write},
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    path::Path,
    slice::Iter,
};
pub struct PdfMetadata<'a> {
    pub(crate) inner: backend::PdfMetadata,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfMetadata<'a> {
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn get(&self, tag: PdfDocumentMetadataTagType) -> Option<PdfDocumentMetadataTag> {
        self.inner.get(tag)
    }
    pub fn iter(&self) -> Iter<'_, PdfDocumentMetadataTag> {
        self.inner.iter()
    }
}
pub type PdfAttachmentIndex = u16;
pub struct PdfAttachments<'a> {
    pub(crate) inner: backend::PdfAttachments,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
pub struct PdfAttachment<'a> {
    pub(crate) inner: backend::PdfAttachment,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfAttachments<'a> {
    pub fn len(&self) -> PdfAttachmentIndex {
        self.inner.len() as PdfAttachmentIndex
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn as_range(&self) -> Range<PdfAttachmentIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfAttachmentIndex> {
        0..=self.len().saturating_sub(1)
    }
    pub fn get(&self, index: PdfAttachmentIndex) -> Result<PdfAttachment<'a>, PdfiumError> {
        Ok(PdfAttachment {
            inner: self.inner.get(index as usize)?,
            lifetime: PhantomData,
        })
    }
    pub fn create_attachment_from_bytes(
        &mut self,
        name: &str,
        bytes: &[u8],
    ) -> Result<PdfAttachment<'_>, PdfiumError> {
        Ok(PdfAttachment {
            inner: self.inner.create_attachment_from_bytes(name, bytes)?,
            lifetime: PhantomData,
        })
    }
    pub fn create_attachment_from_file(
        &mut self,
        name: &str,
        path: &(impl AsRef<Path> + ?Sized),
    ) -> Result<PdfAttachment<'_>, PdfiumError> {
        Ok(PdfAttachment {
            inner: self.inner.create_attachment_from_file(name, path)?,
            lifetime: PhantomData,
        })
    }
    pub fn create_attachment_from_reader<R: Read>(
        &mut self,
        name: &str,
        mut reader: R,
    ) -> Result<PdfAttachment<'_>, PdfiumError> {
        Ok(PdfAttachment {
            inner: self
                .inner
                .create_attachment_from_reader(name, &mut reader)?,
            lifetime: PhantomData,
        })
    }
    pub fn delete_at_index(&mut self, index: PdfAttachmentIndex) -> Result<(), PdfiumError> {
        self.inner.delete_at_index(index as usize)
    }
    pub fn iter(&self) -> PdfAttachmentsIterator<'_> {
        PdfAttachmentsIterator {
            attachments: self,
            next_index: 0,
        }
    }
}
impl<'a> PdfAttachment<'a> {
    pub fn name(&self) -> String {
        self.inner.name()
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn save_to_bytes(&self) -> Result<Vec<u8>, PdfiumError> {
        self.inner.save_to_bytes()
    }
    pub fn save_to_writer<W: Write>(&self, writer: &mut W) -> Result<(), PdfiumError> {
        self.inner.save_to_writer(writer)
    }
    pub fn save_to_file(&self, path: &(impl AsRef<Path> + ?Sized)) -> Result<(), PdfiumError> {
        self.inner.save_to_file(path)
    }
}
pub struct PdfAttachmentsIterator<'a> {
    attachments: &'a PdfAttachments<'a>,
    next_index: PdfAttachmentIndex,
}
impl<'a> Iterator for PdfAttachmentsIterator<'a> {
    type Item = PdfAttachment<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.attachments.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.attachments.get(i).ok()
    }
}
pub struct PdfBookmarks<'a> {
    pub(crate) inner: backend::PdfBookmarks,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
#[derive(Clone)]
pub struct PdfBookmark<'a> {
    pub(crate) inner: backend::PdfBookmark,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfBookmark<'a> {
    pub(crate) fn wrap(inner: backend::PdfBookmark) -> Self {
        Self {
            inner,
            lifetime: PhantomData,
        }
    }
}
impl PartialEq for PdfBookmark<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl Eq for PdfBookmark<'_> {}
impl std::hash::Hash for PdfBookmark<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}
impl<'a> PdfBookmarks<'a> {
    pub fn root(&self) -> Option<PdfBookmark<'_>> {
        self.inner.root().map(PdfBookmark::wrap)
    }
    pub fn find_first_by_title(&self, title: &str) -> Result<PdfBookmark<'_>, PdfiumError> {
        self.inner.find_first_by_title(title).map(PdfBookmark::wrap)
    }
    pub fn find_all_by_title(&self, title: &str) -> Vec<PdfBookmark<'_>> {
        self.inner
            .find_all_by_title(title)
            .into_iter()
            .map(PdfBookmark::wrap)
            .collect()
    }
    pub fn iter(&self) -> PdfBookmarksIterator<'_> {
        PdfBookmarksIterator {
            inner: self
                .inner
                .iter()
                .map(PdfBookmark::wrap)
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }
}
impl<'a> PdfBookmark<'a> {
    pub fn title(&self) -> Option<String> {
        self.inner.title()
    }
    pub fn action(&self) -> Option<PdfAction<'a>> {
        let (document, dict) = self.inner.core_context();
        PdfAction::from_core(
            document,
            pdfoxium_core::fpdfdoc::cpdf_bookmark::CpdfBookmark::new(Some(dict)).action(),
        )
    }
    pub fn destination(&self) -> Option<PdfDestination<'a>> {
        let (document, dict) = self.inner.core_context();
        let dest = pdfoxium_core::fpdfdoc::cpdf_bookmark::CpdfBookmark::new(Some(dict))
            .dest(&document.core.borrow());
        PdfDestination::from_core(document, dest)
    }
    pub fn parent(&self) -> Option<PdfBookmark<'a>> {
        self.inner.parent().map(PdfBookmark::wrap)
    }
    pub fn children_len(&self) -> usize {
        self.inner.children_len()
    }
    pub fn first_child(&self) -> Option<PdfBookmark<'a>> {
        self.inner.first_child().map(PdfBookmark::wrap)
    }
    pub fn next_sibling(&self) -> Option<PdfBookmark<'a>> {
        self.inner.next_sibling().map(PdfBookmark::wrap)
    }
    pub fn iter_siblings(&self) -> PdfBookmarksIterator<'a> {
        PdfBookmarksIterator {
            inner: self
                .inner
                .iter_siblings()
                .map(PdfBookmark::wrap)
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }
    pub fn iter_direct_children(&self) -> PdfBookmarksIterator<'a> {
        PdfBookmarksIterator {
            inner: self
                .inner
                .iter_direct_children()
                .map(PdfBookmark::wrap)
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }
    pub fn iter_all_descendants(&self) -> PdfBookmarksIterator<'a> {
        PdfBookmarksIterator {
            inner: self
                .inner
                .iter_all_descendants()
                .map(PdfBookmark::wrap)
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }
}
pub struct PdfBookmarksIterator<'a> {
    inner: std::vec::IntoIter<PdfBookmark<'a>>,
}
impl<'a> Iterator for PdfBookmarksIterator<'a> {
    type Item = PdfBookmark<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}
