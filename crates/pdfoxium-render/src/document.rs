// Interfaces copied from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::backend::{self, document::DocumentState};
use crate::prelude::*;
use pdfoxium_core::{
    fpdfapi::edit::cpdf_creator::CpdfCreator,
    fpdfsdk::{fpdf_flatten, fpdf_ppo, fpdf_thumbnail, fpdf_transformpage},
    fxcrt::fx_coordinates::{CfxPointF, FxRect},
};
use std::{
    collections::HashMap,
    io::Write,
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    path::Path,
    rc::Rc,
};
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfDocumentVersion {
    /// No version information is available. This is the case if the [PdfDocument]
    /// was created via a call to `Pdfium::create_new_pdf()` rather than loaded from a file.
    Unset,

    /// PDF 1.0, first published in 1993, supported by Acrobat Reader Carousel (1.0) onwards.
    Pdf1_0,

    /// PDF 1.1, first published in 1994, supported by Acrobat Reader 2.0 onwards.
    Pdf1_1,

    /// PDF 1.2, first published in 1996, supported by Acrobat Reader 3.0 onwards.
    Pdf1_2,

    /// PDF 1.3, first published in 2000, supported by Acrobat Reader 4.0 onwards.
    Pdf1_3,

    /// PDF 1.4, first published in 2001, supported by Acrobat Reader 5.0 onwards.
    Pdf1_4,

    /// PDF 1.5, first published in 2003, supported by Acrobat Reader 6.0 onwards.
    Pdf1_5,

    /// PDF 1.6, first published in 2004, supported by Acrobat Reader 7.0 onwards.
    Pdf1_6,

    /// PDF 1.7, first published in 2006, supported by Acrobat Reader 8.0 onwards,
    /// adopted as ISO open standard 32000-1 in 2008. Certain proprietary Adobe
    /// extensions to PDF 1.7 are only fully supported in Acrobat Reader X (10.0)
    /// and later.
    Pdf1_7,

    /// PDF 2.0, first published in 2017, ISO open standard 32000-2.
    Pdf2_0,

    /// A two-digit raw file version number. For instance, a value of 21 would indicate
    /// PDF version 2.1, a value of 34 would indicate PDF version 3.4, and so on.
    /// Only used when the file version number is not directly recognized by
    /// pdfium-render.
    Other(i32),
}
impl PdfDocumentVersion {
    /// The default [PdfDocumentVersion] applied to new documents.
    pub const DEFAULT_VERSION: PdfDocumentVersion = PdfDocumentVersion::Pdf1_7;

    #[inline]
    pub(crate) fn from_pdfium(version: i32) -> Self {
        match version {
            10 => PdfDocumentVersion::Pdf1_0,
            11 => PdfDocumentVersion::Pdf1_1,
            12 => PdfDocumentVersion::Pdf1_2,
            13 => PdfDocumentVersion::Pdf1_3,
            14 => PdfDocumentVersion::Pdf1_4,
            15 => PdfDocumentVersion::Pdf1_5,
            16 => PdfDocumentVersion::Pdf1_6,
            17 => PdfDocumentVersion::Pdf1_7,
            20 => PdfDocumentVersion::Pdf2_0,
            _ => PdfDocumentVersion::Other(version),
        }
    }

    #[inline]
    pub(crate) fn as_pdfium(&self) -> Option<i32> {
        match self {
            PdfDocumentVersion::Pdf1_0 => Some(10),
            PdfDocumentVersion::Pdf1_1 => Some(11),
            PdfDocumentVersion::Pdf1_2 => Some(12),
            PdfDocumentVersion::Pdf1_3 => Some(13),
            PdfDocumentVersion::Pdf1_4 => Some(14),
            PdfDocumentVersion::Pdf1_5 => Some(15),
            PdfDocumentVersion::Pdf1_6 => Some(16),
            PdfDocumentVersion::Pdf1_7 => Some(17),
            PdfDocumentVersion::Pdf2_0 => Some(20),
            PdfDocumentVersion::Other(value) => Some(*value),
            PdfDocumentVersion::Unset => None,
        }
    }
}
/// A loaded or newly created PDF with pages, resources and document metadata.
///
/// The lifetime follows the engine/input used to create it. Page and object
/// wrappers retain shared core state; the document is neither `Send` nor `Sync`.
/// Writes are explicit through [`Self::save_to_file`] or [`Self::save_to_bytes`].
pub struct PdfDocument<'a> {
    pub(crate) inner: backend::PdfDocument,
    output_version: Option<PdfDocumentVersion>,
    attachments: PdfAttachments<'a>,
    bookmarks: std::cell::OnceCell<PdfBookmarks<'a>>,
    catalog: PdfCatalog<'a>,
    form: Option<PdfForm<'a>>,
    fonts: PdfFonts<'a>,
    metadata: PdfMetadata<'a>,
    pages: PdfPages<'a>,
    permissions: PdfPermissions<'a>,
    signatures: PdfSignatures<'a>,
}
impl<'a> PdfDocument<'a> {
    pub(crate) fn from_inner(inner: backend::PdfDocument) -> Self {
        let state = inner.inner.clone();
        let form = PdfForm::from_core(state.clone());
        Self {
            attachments: PdfAttachments {
                inner: inner.attachments(),
                lifetime: PhantomData,
            },
            bookmarks: std::cell::OnceCell::new(),
            catalog: PdfCatalog {
                document: state.clone(),
                lifetime: PhantomData,
            },
            form,
            fonts: PdfFonts {
                document: state.clone(),
                fonts: HashMap::new(),
            },
            metadata: PdfMetadata {
                inner: inner.metadata(),
                lifetime: PhantomData,
            },
            pages: PdfPages {
                inner: inner.pages(),
                document: state.clone(),
                lifetime: PhantomData,
            },
            permissions: PdfPermissions {
                document: state.clone(),
                lifetime: PhantomData,
            },
            signatures: PdfSignatures {
                document: state,
                lifetime: PhantomData,
            },
            inner,
            output_version: None,
        }
    }
    fn serialize(&self) -> Result<Vec<u8>, PdfiumError> {
        struct Sink(Vec<u8>);
        impl pdfoxium_core::fxcrt::fx_stream::WriteStream for Sink {
            fn write_block(&mut self, bytes: &[u8]) -> bool {
                self.0.extend_from_slice(bytes);
                true
            }
        }
        let mut sink = Sink(Vec::new());
        let version = self
            .output_version
            .map(|v| v.as_pdfium().unwrap_or(0))
            .unwrap_or(0);
        if !CpdfCreator::new(self.inner.inner.core.clone(), &mut sink)
            .create(CpdfCreator::NONE, version)
        {
            return Err(PdfiumError::PdfiumLibraryInternalError(
                PdfiumInternalError::Unknown,
            ));
        }
        Ok(sink.0)
    }
}
impl<'a> PdfDocument<'a> {
    pub fn version(&self) -> PdfDocumentVersion {
        let document = self.inner.inner.core.borrow();
        if document.parser().document_size().is_none() {
            PdfDocumentVersion::Unset
        } else {
            PdfDocumentVersion::from_pdfium(document.parser().file_version())
        }
    }
    pub fn set_version(&mut self, version: PdfDocumentVersion) {
        self.output_version = Some(version)
    }
    pub fn attachments(&self) -> &PdfAttachments<'_> {
        &self.attachments
    }
    pub fn attachments_mut(&mut self) -> &mut PdfAttachments<'a> {
        &mut self.attachments
    }
    pub fn bookmarks(&self) -> &PdfBookmarks<'_> {
        self.bookmarks.get_or_init(|| PdfBookmarks {
            inner: self.inner.bookmarks(),
            lifetime: PhantomData,
        })
    }
    pub fn catalog(&self) -> &PdfCatalog<'_> {
        &self.catalog
    }
    pub fn catalog_mut(&mut self) -> &mut PdfCatalog<'a> {
        &mut self.catalog
    }
    pub fn form(&self) -> Option<&PdfForm<'_>> {
        self.form.as_ref()
    }
    pub fn fonts(&self) -> &PdfFonts<'_> {
        &self.fonts
    }
    pub fn fonts_mut(&mut self) -> &mut PdfFonts<'a> {
        &mut self.fonts
    }
    pub fn metadata(&self) -> &PdfMetadata<'_> {
        &self.metadata
    }
    pub fn pages(&self) -> &PdfPages<'a> {
        &self.pages
    }
    pub fn pages_mut(&mut self) -> &mut PdfPages<'a> {
        &mut self.pages
    }
    pub fn permissions(&self) -> &PdfPermissions<'_> {
        &self.permissions
    }
    pub fn signatures(&self) -> &PdfSignatures<'_> {
        &self.signatures
    }
    pub fn save_to_writer<W: Write + 'static>(&self, writer: &mut W) -> Result<(), PdfiumError> {
        writer
            .write_all(&self.serialize()?)
            .map_err(PdfiumError::IoError)?;
        writer.flush().map_err(PdfiumError::IoError)
    }
    pub fn save_to_file(&self, path: &(impl AsRef<Path> + ?Sized)) -> Result<(), PdfiumError> {
        self.save_to_writer(&mut std::fs::File::create(path).map_err(PdfiumError::IoError)?)
    }
    pub fn save_to_bytes(&self) -> Result<Vec<u8>, PdfiumError> {
        self.serialize()
    }
}
pub type PdfPageIndex = std::os::raw::c_int;
#[derive(Debug, Copy, Clone)]
pub enum PdfPageMode {
    /// No known page mode is set for this [PdfDocument].
    UnsetOrUnknown = -1 as isize,

    /// No page mode, i.e. neither the document outline nor thumbnail images should be visible,
    /// no side panels should be visible, and the document should not be displayed in full screen mode.
    None = 0 as isize,

    /// Outline page mode: the document outline should be visible.
    ShowDocumentOutline = 1 as isize,

    /// Thumbnail page mode: page thumbnails should be visible.
    ShowPageThumbnails = 2 as isize,

    /// Fullscreen page mode: no menu bar, window controls, or other windows should be visible.
    Fullscreen = 3 as isize,

    /// The optional content group panel should be visible.
    ShowContentGroupPanel = 4 as isize,

    /// The attachments panel should be visible.
    ShowAttachmentsPanel = 5 as isize,
}
/// Zero-based page collection, accessed through [`PdfDocument::pages`].
///
/// Use the mutable collection from [`PdfDocument::pages_mut`] to create, delete
/// or import pages. Page deletion invalidates existing handles to that page.
pub struct PdfPages<'a> {
    pub(crate) inner: backend::PdfPages,
    pub(crate) document: Rc<DocumentState>,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPages<'a> {
    pub fn len(&self) -> PdfPageIndex {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn as_range(&self) -> Range<PdfPageIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfPageIndex> {
        0..=self.len().saturating_sub(1)
    }
    pub fn get(&self, index: PdfPageIndex) -> Result<PdfPage<'a>, PdfiumError> {
        self.inner.get(index).map(PdfPage::from_inner)
    }
    pub fn page_size(&self, index: PdfPageIndex) -> Result<PdfRect, PdfiumError> {
        self.inner.page_size(index)
    }
    pub fn page_sizes(&self) -> Result<Vec<PdfRect>, PdfiumError> {
        self.inner.page_sizes()
    }
    pub fn first(&self) -> Result<PdfPage<'a>, PdfiumError> {
        self.get(0)
    }
    pub fn last(&self) -> Result<PdfPage<'a>, PdfiumError> {
        if self.is_empty() {
            Err(PdfiumError::NoPagesInDocument)
        } else {
            self.get(self.len() - 1)
        }
    }
    pub fn create_page_at_start(
        &mut self,
        size: PdfPagePaperSize,
    ) -> Result<PdfPage<'a>, PdfiumError> {
        self.inner
            .create_page_at_start(size)
            .map(PdfPage::from_inner)
    }
    pub fn create_page_at_end(
        &mut self,
        size: PdfPagePaperSize,
    ) -> Result<PdfPage<'a>, PdfiumError> {
        self.inner.create_page_at_end(size).map(PdfPage::from_inner)
    }
    pub fn create_page_at_index(
        &mut self,
        size: PdfPagePaperSize,
        index: PdfPageIndex,
    ) -> Result<PdfPage<'a>, PdfiumError> {
        self.inner
            .create_page_at_index(size, index)
            .map(PdfPage::from_inner)
    }
    pub fn copy_page_from_document(
        &mut self,
        source: &PdfDocument,
        source_page_index: PdfPageIndex,
        destination_page_index: PdfPageIndex,
    ) -> Result<(), PdfiumError> {
        self.inner.copy_pages_from_document(
            &source.inner,
            &[source_page_index],
            destination_page_index,
        )
    }
    pub fn copy_pages_from_document(
        &mut self,
        source: &PdfDocument,
        pages: &str,
        destination_page_index: PdfPageIndex,
    ) -> Result<(), PdfiumError> {
        let selected = parse_page_range(pages, source.pages().len())?;
        self.inner
            .copy_pages_from_document(&source.inner, &selected, destination_page_index)
    }
    pub fn copy_page_range_from_document(
        &mut self,
        source: &PdfDocument,
        source_page_range: RangeInclusive<PdfPageIndex>,
        destination_page_index: PdfPageIndex,
    ) -> Result<(), PdfiumError> {
        self.inner.copy_page_range_from_document(
            &source.inner,
            source_page_range,
            destination_page_index,
        )
    }
    pub fn append(&mut self, document: &PdfDocument) -> Result<(), PdfiumError> {
        self.inner.append(&document.inner)
    }
    pub fn tile_into_new_document(
        &self,
        rows_per_page: u8,
        columns_per_row: u8,
        size: PdfPagePaperSize,
    ) -> Result<PdfDocument<'_>, PdfiumError> {
        let destination =
            pdfoxium_core::fpdfsdk::fpdf_editpage::create_new_document(false, || None);
        if !fpdf_ppo::import_n_pages_to_one(
            destination.clone(),
            self.document.core.clone(),
            size.width().value,
            size.height().value,
            columns_per_row.into(),
            rows_per_page.into(),
        ) {
            return Err(PdfiumError::PdfiumLibraryInternalError(
                PdfiumInternalError::Unknown,
            ));
        }
        Ok(PdfDocument::from_inner(backend::PdfDocument::from_core(
            destination,
        )))
    }
    pub fn page_mode(&self) -> PdfPageMode {
        let document = self.document.core.borrow();
        let value = document
            .root()
            .and_then(|r| {
                r.borrow()
                    .as_dictionary()
                    .map(|d| d.get_name_for(b"PageMode"))
            })
            .unwrap_or_default();
        match value.as_slice() {
            b"UseNone" => PdfPageMode::None,
            b"UseOutlines" => PdfPageMode::ShowDocumentOutline,
            b"UseThumbs" => PdfPageMode::ShowPageThumbnails,
            b"FullScreen" => PdfPageMode::Fullscreen,
            b"UseOC" => PdfPageMode::ShowContentGroupPanel,
            b"UseAttachments" => PdfPageMode::ShowAttachmentsPanel,
            _ => PdfPageMode::UnsetOrUnknown,
        }
    }
    pub fn watermark<F>(&self, watermarker: F) -> Result<(), PdfiumError>
    where
        F: Fn(
            &mut PdfPageGroupObject<'a>,
            PdfPageIndex,
            PdfPoints,
            PdfPoints,
        ) -> Result<(), PdfiumError>,
    {
        for index in self.as_range() {
            let page = self.get(index)?;
            let mut group = PdfPageGroupObject::from_page(page.inner.alias());
            watermarker(&mut group, index, page.width(), page.height())?;
        }
        Ok(())
    }
    pub fn iter(&self) -> PdfPagesIterator<'_> {
        PdfPagesIterator {
            pages: self,
            next_index: 0,
        }
    }
}
pub struct PdfPagesIterator<'a> {
    pages: &'a PdfPages<'a>,
    next_index: PdfPageIndex,
}
impl<'a> Iterator for PdfPagesIterator<'a> {
    type Item = PdfPage<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.pages.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.pages.get(i).ok()
    }
}
fn parse_page_range(text: &str, count: PdfPageIndex) -> Result<Vec<PdfPageIndex>, PdfiumError> {
    if text.is_empty() {
        return Ok((0..count).collect());
    }
    let mut result = Vec::new();
    for part in text.split(',') {
        let mut bounds = part.trim().split('-');
        let first = bounds
            .next()
            .and_then(|v| v.trim().parse::<i32>().ok())
            .ok_or(PdfiumError::PageIndexOutOfBounds)?;
        let last = match bounds.next() {
            None => first,
            Some(value) => value
                .trim()
                .parse::<i32>()
                .map_err(|_| PdfiumError::PageIndexOutOfBounds)?,
        };
        if bounds.next().is_some() || first <= 0 || last < first || last > count {
            return Err(PdfiumError::PageIndexOutOfBounds);
        }
        result.extend((first - 1)..last);
    }
    Ok(result)
}
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PdfPageContentRegenerationStrategy {
    /// `pdfium-render` will call the [PdfPage::regenerate_content()] function on any
    /// change to this [PdfPage]. This is the default setting.
    AutomaticOnEveryChange,

    /// `pdfium-render` will call the [PdfPage::regenerate_content()] function only when
    /// this [PdfPage] is about to move out of scope.
    AutomaticOnDrop,

    /// `pdfium-render` will never call the [PdfPage::regenerate_content()] function.
    /// You must do so manually after staging your changes, or your changes will be lost
    /// when this [PdfPage] moves out of scope.
    Manual,
}
/// A live PDF page with rendering, text, objects and annotations.
///
/// Obtain pages through [`PdfPages::get`] or [`PdfPages::iter`]. Use
/// [`Self::render_with_config`] for owned rendered pixels and [`Self::text`] for
/// text queries. Invalidated page handles return an error when accessed.
pub struct PdfPage<'a> {
    pub(crate) inner: backend::PdfPage,
    label: Option<String>,
    objects: PdfPageObjects<'a>,
    annotations: PdfPageAnnotations<'a>,
    boundaries: PdfPageBoundaries<'a>,
    links: PdfPageLinks<'a>,
    structure_tree: PdfPageStructureTree<'a>,
}
pub struct PdfPageStructureTree<'a> {
    _tree: Option<Rc<pdfoxium_core::fpdfdoc::cpdf_structtree::CpdfStructTree>>,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPage<'a> {
    pub(crate) fn from_inner(inner: backend::PdfPage) -> Self {
        let label = pdfoxium_core::fpdfdoc::cpdf_pagelabel::CpdfPageLabel::new(Some(
            &inner.document.core.borrow(),
        ))
        .get_label(inner.index())
        .map(|v| backend::text::decode(&v));
        let tree = pdfoxium_core::fpdfdoc::cpdf_structtree::CpdfStructTree::load_page(
            &inner.document.core.borrow(),
            inner.state.core.borrow().dict(),
        );
        Self {
            label,
            objects: PdfPageObjects::new(inner.alias()),
            annotations: PdfPageAnnotations::new(inner.alias()),
            boundaries: PdfPageBoundaries::new(inner.alias()),
            links: PdfPageLinks::new(inner.alias()),
            structure_tree: PdfPageStructureTree {
                _tree: tree,
                lifetime: PhantomData,
            },
            inner,
        }
    }
}
impl<'a> PdfPage<'a> {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
    pub fn width(&self) -> PdfPoints {
        self.inner.width()
    }
    pub fn height(&self) -> PdfPoints {
        self.inner.height()
    }
    pub fn page_size(&self) -> PdfRect {
        self.inner.page_size()
    }
    pub fn orientation(&self) -> PdfPageOrientation {
        if self.is_landscape() {
            PdfPageOrientation::Landscape
        } else {
            PdfPageOrientation::Portrait
        }
    }
    pub fn is_portrait(&self) -> bool {
        self.inner.is_portrait()
    }
    pub fn is_landscape(&self) -> bool {
        self.inner.is_landscape()
    }
    pub fn rotation(&self) -> Result<PdfPageRenderRotation, PdfiumError> {
        self.inner.rotation()
    }
    pub fn set_rotation(&mut self, rotation: PdfPageRenderRotation) {
        self.inner.set_rotation(rotation).expect("live PDF page")
    }
    pub fn has_transparency(&self) -> bool {
        self.inner.state.core.borrow().transparency().is_group()
            || self.objects.iter().any(|o| o.has_transparency())
    }
    pub fn paper_size(&self) -> PdfPagePaperSize {
        PdfPagePaperSize::from_points(self.width(), self.height())
    }
    pub fn has_embedded_thumbnail(&self) -> bool {
        fpdf_thumbnail::with_thumbnail_data(&self.inner.state.core.borrow(), false, |data| {
            !data.is_empty()
        })
        .unwrap_or(false)
    }
    pub fn embedded_thumbnail(&self) -> Result<PdfBitmap<'_>, PdfiumError> {
        let bitmap = fpdf_thumbnail::thumbnail_as_bitmap(&self.inner.state.core.borrow())
            .ok_or(PdfiumError::PageMissingEmbeddedThumbnail)?;
        PdfBitmap::from_core(bitmap)
    }
    pub fn text(&self) -> Result<PdfPageText<'_>, PdfiumError> {
        PdfPageText::from_page(self.inner.alias())
    }
    pub fn annotations(&self) -> &PdfPageAnnotations<'_> {
        &self.annotations
    }
    pub fn annotations_mut(&mut self) -> &mut PdfPageAnnotations<'a> {
        &mut self.annotations
    }
    pub fn boundaries(&self) -> &PdfPageBoundaries<'_> {
        &self.boundaries
    }
    pub fn boundaries_mut(&mut self) -> &mut PdfPageBoundaries<'a> {
        &mut self.boundaries
    }
    pub fn links(&self) -> &PdfPageLinks<'_> {
        &self.links
    }
    pub fn links_mut(&mut self) -> &mut PdfPageLinks<'a> {
        &mut self.links
    }
    pub fn objects(&self) -> &PdfPageObjects<'_> {
        &self.objects
    }
    pub fn objects_mut(&mut self) -> &mut PdfPageObjects<'a> {
        &mut self.objects
    }
    pub fn structure_tree(&self) -> &PdfPageStructureTree<'_> {
        &self.structure_tree
    }
    pub fn fonts(&self) -> Vec<PdfFont<'_>> {
        self.objects.iter().filter_map(|o|o.handle().read(|o|Ok(o.as_any().downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_textobject::CpdfTextObject>().and_then(|t|t.font()))).ok().flatten()).map(|font|PdfFont::from_existing(self.inner.document.clone(),font)).collect()
    }
    pub fn pixels_to_points(
        &self,
        x: Pixels,
        y: Pixels,
        config: &PdfRenderConfig,
    ) -> Result<(PdfPoints, PdfPoints), PdfiumError> {
        let s = config.apply_to_page(&self.inner);
        let rect = FxRect::new(
            s.start_x,
            s.start_y,
            s.start_x
                .checked_add(s.width)
                .ok_or(PdfiumError::CoordinateConversionFunctionIndicatedError)?,
            s.start_y
                .checked_add(s.height)
                .ok_or(PdfiumError::CoordinateConversionFunctionIndicatedError)?,
        );
        let point = self.inner.state.core.borrow().device_to_page(
            rect,
            s.rotate,
            CfxPointF::new(x as f32, y as f32),
        );
        Ok((PdfPoints::new(point.x), PdfPoints::new(point.y)))
    }
    pub fn points_to_pixels(
        &self,
        x: PdfPoints,
        y: PdfPoints,
        config: &PdfRenderConfig,
    ) -> Result<(Pixels, Pixels), PdfiumError> {
        let s = config.apply_to_page(&self.inner);
        let rect = FxRect::new(
            s.start_x,
            s.start_y,
            s.start_x
                .checked_add(s.width)
                .ok_or(PdfiumError::CoordinateConversionFunctionIndicatedError)?,
            s.start_y
                .checked_add(s.height)
                .ok_or(PdfiumError::CoordinateConversionFunctionIndicatedError)?,
        );
        let point = self.inner.state.core.borrow().page_to_device(
            rect,
            s.rotate,
            CfxPointF::new(x.value, y.value),
        );
        Ok((point.x.round() as Pixels, point.y.round() as Pixels))
    }
    pub fn render(
        &self,
        width: Pixels,
        height: Pixels,
        rotation: Option<PdfPageRenderRotation>,
    ) -> Result<PdfBitmap<'_>, PdfiumError> {
        self.inner.render(width, height, rotation)
    }
    pub fn render_with_config(
        &self,
        config: &PdfRenderConfig,
    ) -> Result<PdfBitmap<'_>, PdfiumError> {
        self.inner.render_with_config(config)
    }
    pub fn render_into_bitmap(
        &self,
        bitmap: &mut PdfBitmap,
        width: Pixels,
        height: Pixels,
        rotation: Option<PdfPageRenderRotation>,
    ) -> Result<(), PdfiumError> {
        self.inner.render_into_bitmap_with_config(
            bitmap,
            &PdfRenderConfig::new()
                .set_target_size(width, height)
                .rotate(rotation.unwrap_or_default(), false),
        )
    }
    pub fn render_into_bitmap_with_config(
        &self,
        bitmap: &mut PdfBitmap,
        config: &PdfRenderConfig,
    ) -> Result<(), PdfiumError> {
        self.inner.render_into_bitmap_with_config(bitmap, config)
    }
    pub fn transform_with_clip(
        &mut self,
        a: PdfMatrixValue,
        b: PdfMatrixValue,
        c: PdfMatrixValue,
        d: PdfMatrixValue,
        e: PdfMatrixValue,
        f: PdfMatrixValue,
        clip: PdfRect,
    ) -> Result<(), PdfiumError> {
        self.inner.check_live()?;
        let result = {
            let page = self.inner.state.core.borrow();
            fpdf_transformpage::transform_with_clip(
                &self.inner.document.core.borrow(),
                &page,
                Some(PdfMatrix::new(a, b, c, d, e, f).as_core()),
                Some(clip.as_core()),
            )
        };
        self.inner.reload_core()?;
        if result {
            Ok(())
        } else {
            Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
        }
    }
    pub fn apply_matrix_with_clip(
        &mut self,
        matrix: PdfMatrix,
        clip: PdfRect,
    ) -> Result<(), PdfiumError> {
        self.transform_with_clip(
            matrix.a(),
            matrix.b(),
            matrix.c(),
            matrix.d(),
            matrix.e(),
            matrix.f(),
            clip,
        )
    }
    pub fn flatten(&mut self) -> Result<(), PdfiumError> {
        let status = {
            let page = self.inner.state.core.borrow();
            fpdf_flatten::flatten(&self.inner.document.core, &page, fpdf_flatten::FLAT_PRINT)
        };
        if status == fpdf_flatten::FLATTEN_NOTHINGTODO {
            return Ok(());
        }
        self.inner.reload_core()?;
        if status == fpdf_flatten::FLATTEN_SUCCESS {
            Ok(())
        } else {
            Err(PdfiumError::PageFlattenFailure)
        }
    }
    pub fn delete(self) -> Result<(), PdfiumError> {
        let index = self.inner.index();
        let document = backend::PdfDocument {
            inner: self.inner.document.clone(),
        };
        document.pages().delete_page_at_index(index)
    }
    pub fn content_regeneration_strategy(&self) -> PdfPageContentRegenerationStrategy {
        self.inner.state.strategy.get()
    }
    pub fn set_content_regeneration_strategy(
        &mut self,
        strategy: PdfPageContentRegenerationStrategy,
    ) {
        self.inner.state.strategy.set(strategy)
    }
    pub fn regenerate_content(&mut self) -> Result<(), PdfiumError> {
        self.inner.regenerate_content()
    }
}
impl Drop for PdfPage<'_> {
    fn drop(&mut self) {
        if self.content_regeneration_strategy()
            == PdfPageContentRegenerationStrategy::AutomaticOnDrop
        {
            let _ = self.inner.regenerate_content();
        }
    }
}

impl std::fmt::Debug for PdfDocument<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfDocument")
            .field("version", &self.version())
            .field("page_count", &self.pages().len())
            .finish()
    }
}
