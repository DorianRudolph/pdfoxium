// Interfaces copied from pdfium-render 0.9.4; MIT.
use crate::prelude::*;
use crate::{backend, object_store::ObjectHandle};
#[cfg(feature = "image_api")]
use image::DynamicImage;
use pdfoxium_core::fpdfapi::{
    page::{cpdf_annotcontext::CpdfAnnotContext, cpdf_formobject::CpdfFormObject},
    parser::cpdf_object::ObjectRef,
};
use std::{
    cell::RefCell,
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};
pub type PdfPageObjectIndex = usize;
pub trait PdfPageObjectsCommon<'a> {
    /// Returns the total number of page objects in the collection.
    fn len(&self) -> PdfPageObjectIndex;

    /// Returns true if this page objects collection is empty.
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a Range from `0..(number of objects)` for this page objects collection.
    #[inline]
    fn as_range(&self) -> Range<PdfPageObjectIndex> {
        0..self.len()
    }

    /// Returns an inclusive Range from `0..=(number of objects - 1)` for this page objects collection.
    #[inline]
    fn as_range_inclusive(&self) -> RangeInclusive<PdfPageObjectIndex> {
        if self.is_empty() {
            0..=0
        } else {
            0..=(self.len() - 1)
        }
    }

    /// Returns a single [PdfPageObject] from this page objects collection.
    fn get(&self, index: PdfPageObjectIndex) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Returns the first [PdfPageObject] in this page objects collection.
    #[inline]
    fn first(&self) -> Result<PdfPageObject<'a>, PdfiumError> {
        if !self.is_empty() {
            self.get(0)
        } else {
            Err(PdfiumError::NoPageObjectsInCollection)
        }
    }

    /// Returns the last [PdfPageObject] in this page objects collection.
    #[inline]
    fn last(&self) -> Result<PdfPageObject<'a>, PdfiumError> {
        if !self.is_empty() {
            self.get(self.len() - 1)
        } else {
            Err(PdfiumError::NoPageObjectsInCollection)
        }
    }

    /// Returns an iterator over all the [PdfPageObject] objects in this page objects collection.
    fn iter(&'a self) -> PdfPageObjectsIterator<'a>;

    /// Returns the smallest bounding box that contains all the [PdfPageObject] objects in this
    /// page objects collection.
    fn bounds(&'a self) -> PdfRect {
        let mut bottom: f32 = 0.0;
        let mut top: f32 = 0.0;
        let mut left: f32 = 0.0;
        let mut right: f32 = 0.0;

        for object in self.iter() {
            if let Ok(bounds) = object.bounds() {
                bottom = bottom.min(bounds.bottom().value);
                top = top.max(bounds.top().value);
                left = left.min(bounds.left().value);
                right = right.max(bounds.right().value);
            }
        }

        PdfRect::new_from_values(bottom, left, top, right)
    }

    /// Adds the given [PdfPageObject] to this page objects collection. The object's
    /// memory ownership will be transferred to the [PdfPage] containing this page objects
    /// collection, and the updated page object will be returned.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    fn add_object(&mut self, object: PdfPageObject<'a>) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Adds the given [PdfPageTextObject] to this page objects collection,
    /// returning the text object wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    #[inline]
    fn add_text_object(
        &mut self,
        object: PdfPageTextObject<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        self.add_object(PdfPageObject::Text(object))
    }

    /// Creates a new [PdfPageTextObject] at the given x and y page co-ordinates
    /// from the given arguments and adds it to this page objects collection,
    /// returning the text object wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    fn create_text_object(
        &mut self,
        x: PdfPoints,
        y: PdfPoints,
        text: impl ToString,
        font: impl ToPdfFontToken,
        font_size: PdfPoints,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Adds the given [PdfPagePathObject] to this page objects collection,
    /// returning the path object wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    #[inline]
    fn add_path_object(
        &mut self,
        object: PdfPagePathObject<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        self.add_object(PdfPageObject::Path(object))
    }

    /// Adds the given [PdfPageXObjectFormObject] to this page objects collection,
    /// returning the XObject form object wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// If the containing `PdfPage` has a content regeneration strategy of
    /// `PdfPageContentRegenerationStrategy::AutomaticOnEveryChange` then content regeneration
    /// will be triggered on the page.
    #[inline]
    fn add_x_object_form_object(
        &mut self,
        object: PdfPageXObjectFormObject<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        self.add_object(PdfPageObject::XObjectForm(object))
    }

    /// Creates a new [PdfPagePathObject] for the given line, with the given
    /// stroke settings applied. The new path object will be added to this page objects collection
    /// and then returned, wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    fn create_path_object_line(
        &mut self,
        x1: PdfPoints,
        y1: PdfPoints,
        x2: PdfPoints,
        y2: PdfPoints,
        stroke_color: PdfColor,
        stroke_width: PdfPoints,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Creates a new [PdfPagePathObject] for the given cubic Bézier curve, with the given
    /// stroke settings applied. The new path object will be added to this page objects collection
    /// and then returned, wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    #[allow(clippy::too_many_arguments)]
    fn create_path_object_bezier(
        &mut self,
        x1: PdfPoints,
        y1: PdfPoints,
        x2: PdfPoints,
        y2: PdfPoints,
        control1_x: PdfPoints,
        control1_y: PdfPoints,
        control2_x: PdfPoints,
        control2_y: PdfPoints,
        stroke_color: PdfColor,
        stroke_width: PdfPoints,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Creates a new [PdfPagePathObject] for the given rectangle, with the given
    /// fill and stroke settings applied. Both the stroke color and the stroke width must be
    /// provided for the rectangle to be stroked. The new path object will be added to
    /// this page objects collection and then returned, wrapped inside a generic
    /// [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    fn create_path_object_rect(
        &mut self,
        rect: PdfRect,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Creates a new [PdfPagePathObject]. The new path will be created with a circle that fills
    /// the given rectangle, with the given fill and stroke settings applied. Both the stroke color
    /// and the stroke width must be provided for the circle to be stroked. The new path object
    /// will be added to this page objects collection and then returned, wrapped inside a generic
    /// [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    fn create_path_object_circle(
        &mut self,
        rect: PdfRect,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Creates a new [PdfPagePathObject]. The new path will be created with a circle centered
    /// at the given coordinates, with the given radius, and with the given fill and stroke settings
    /// applied. Both the stroke color and the stroke width must be provided for the circle to be
    /// stroked. The new path object will be added to this page objects collection and then
    /// returned, wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    fn create_path_object_circle_at(
        &mut self,
        center_x: PdfPoints,
        center_y: PdfPoints,
        radius: PdfPoints,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Creates a new [PdfPagePathObject]. The new path will be created with an ellipse that fills
    /// the given rectangle, with the given fill and stroke settings applied. Both the stroke color
    /// and the stroke width must be provided for the ellipse to be stroked. The new path object
    /// will be added to this page objects collection and then returned, wrapped inside a generic
    /// [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    fn create_path_object_ellipse(
        &mut self,
        rect: PdfRect,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Creates a new [PdfPagePathObject]. The new path will be created with an ellipse centered
    /// at the given coordinates, with the given radii, and with the given fill and stroke settings
    /// applied. Both the stroke color and the stroke width must be provided for the ellipse to be
    /// stroked. The new path object will be added to this page objects collection and then
    /// returned, wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    #[allow(clippy::too_many_arguments)]
    fn create_path_object_ellipse_at(
        &mut self,
        center_x: PdfPoints,
        center_y: PdfPoints,
        x_radius: PdfPoints,
        y_radius: PdfPoints,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Adds the given [PdfPageImageObject] to this page objects collection,
    /// returning the image object wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    #[inline]
    fn add_image_object(
        &mut self,
        object: PdfPageImageObject<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        self.add_object(PdfPageObject::Image(object))
    }

    /// Creates a new [PdfPageImageObject] at the given x and y page co-ordinates
    /// from the given arguments and adds it to this page objects collection,
    /// returning the image object wrapped inside a generic [PdfPageObject] wrapper.
    ///
    /// By default, new image objects have their width and height both set to 1.0 points.
    /// If provided, the given width and/or height will be applied to the newly created object to
    /// scale its size.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    ///
    /// This function is only available when this crate's `image` feature is enabled.
    #[cfg(feature = "image_api")]
    fn create_image_object(
        &mut self,
        x: PdfPoints,
        y: PdfPoints,
        image: &DynamicImage,
        width: Option<PdfPoints>,
        height: Option<PdfPoints>,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Removes the given [PdfPageObject] from this page objects collection. The object's
    /// memory ownership will be removed from the [PdfPage] containing this page objects
    /// collection, and the updated page object will be returned. It can be added back to a
    /// page objects collection or dropped, at which point the memory owned by the object will
    /// be freed.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    fn remove_object(
        &mut self,
        object: PdfPageObject<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;

    /// Removes the [PdfPageObject] at the given index from this page objects collection.
    /// The object's memory ownership will be removed from the [PdfPage] containing this page objects
    /// collection, and the updated page object will be returned. It can be added back into a
    /// page objects collection or discarded, at which point the memory owned by the object will
    /// be freed.
    ///
    /// If the containing [PdfPage] has a content regeneration strategy of
    /// [PdfPageContentRegenerationStrategy::AutomaticOnEveryChange] then content regeneration
    /// will be triggered on the page.
    fn remove_object_at_index(
        &mut self,
        index: PdfPageObjectIndex,
    ) -> Result<PdfPageObject<'a>, PdfiumError>;
}
pub(crate) enum ObjectCollection {
    Page(backend::PdfPage),
    Form(ObjectHandle),
    Annotation {
        page: backend::PdfPage,
        dict: ObjectRef,
        context: Rc<RefCell<CpdfAnnotContext>>,
    },
}
impl ObjectCollection {
    pub(crate) fn document(&self) -> Rc<backend::document::DocumentState> {
        match self {
            Self::Page(p) | Self::Annotation { page: p, .. } => p.document.clone(),
            Self::Form(h) => h.document(),
        }
    }
    fn len(&self) -> usize {
        match self {
            Self::Page(p) => p.state.core.borrow().holder().page_object_count(),
            Self::Form(h) => h
                .read(|o| {
                    Ok(o.as_any()
                        .downcast_ref::<CpdfFormObject>()
                        .ok_or(PdfiumError::UnsupportedPdfPageObjectType)?
                        .form()
                        .holder()
                        .page_object_count())
                })
                .unwrap_or(0),
            Self::Annotation { context, .. } => context
                .borrow()
                .form()
                .map_or(0, |f| f.holder().page_object_count()),
        }
    }
    fn get(&self, index: usize) -> Result<ObjectHandle, PdfiumError> {
        match self {
            Self::Page(p) => ObjectHandle::page(p, index),
            Self::Form(h) => ObjectHandle::form(h.clone(), index),
            Self::Annotation {
                page,
                dict,
                context,
            } => ObjectHandle::annotation(page.alias(), dict.clone(), context.clone(), index),
        }
    }
    fn add(&self, handle: &ObjectHandle, index: Option<usize>) -> Result<(), PdfiumError> {
        match self {
            Self::Page(p) => handle.attach_to_page(p, index),
            Self::Form(_) => Err(PdfiumError::PageObjectsCollectionIsImmutable),
            Self::Annotation {
                page,
                dict,
                context,
            } => {
                if !pdfoxium_core::fpdfsdk::fpdf_annot::ensure_append_form(
                    &mut context.borrow_mut(),
                    &page.state.core.borrow(),
                ) {
                    return Err(PdfiumError::UnsupportedPdfPageObjectType);
                }
                handle.attach_to_annotation(page, dict.clone(), context.clone())
            }
        }
    }
    fn remove(&self, handle: &ObjectHandle) -> Result<(), PdfiumError> {
        if matches!(self, Self::Form(_)) {
            return Err(PdfiumError::PageObjectsCollectionIsImmutable);
        }
        if !(0..self.len()).any(|i| self.get(i).is_ok_and(|h| h.same_object(handle))) {
            return Err(PdfiumError::OwnershipNotAttachedToPage);
        }
        handle.remove()
    }
}
pub(crate) mod private {
    pub(crate) trait CollectionOwner {
        fn collection(&self) -> &super::ObjectCollection;
    }
}
pub struct PdfPageObjects<'a> {
    pub(crate) collection: ObjectCollection,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPageObjects<'a> {
    pub(crate) fn new(page: backend::PdfPage) -> Self {
        Self {
            collection: ObjectCollection::Page(page),
            lifetime: PhantomData,
        }
    }
}
impl private::CollectionOwner for PdfPageObjects<'_> {
    fn collection(&self) -> &ObjectCollection {
        &self.collection
    }
}
pub struct PdfPageObjectsIterator<'a> {
    get: Box<dyn Fn(usize) -> Result<PdfPageObject<'a>, PdfiumError> + 'a>,
    next_index: usize,
    length: usize,
}
impl<'a> PdfPageObjectsIterator<'a> {
    pub(crate) fn new<T: PdfPageObjectsCommon<'a> + 'a>(objects: &'a T) -> Self {
        Self {
            get: Box::new(move |i| objects.get(i)),
            next_index: 0,
            length: objects.len(),
        }
    }
    pub(crate) fn from_form(form: &'a PdfPageXObjectFormObject<'a>) -> Self {
        Self {
            get: Box::new(move |i| form.get(i)),
            next_index: 0,
            length: form.len(),
        }
    }
}
impl<'a> Iterator for PdfPageObjectsIterator<'a> {
    type Item = PdfPageObject<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.length {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        (self.get)(i).ok()
    }
}
impl<'a, T: private::CollectionOwner> PdfPageObjectsCommon<'a> for T {
    fn len(&self) -> PdfPageObjectIndex {
        self.collection().len()
    }
    fn get(&self, index: PdfPageObjectIndex) -> Result<PdfPageObject<'a>, PdfiumError> {
        PdfPageObject::from_handle(self.collection().get(index)?)
    }
    fn iter(&'a self) -> PdfPageObjectsIterator<'a> {
        PdfPageObjectsIterator::new(self)
    }
    fn add_object(&mut self, object: PdfPageObject<'a>) -> Result<PdfPageObject<'a>, PdfiumError> {
        self.collection().add(object.handle(), None)?;
        Ok(object)
    }
    fn create_text_object(
        &mut self,
        x: PdfPoints,
        y: PdfPoints,
        text: impl ToString,
        font: impl ToPdfFontToken,
        font_size: PdfPoints,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let document = PdfDocument::from_inner(backend::PdfDocument {
            inner: self.collection().document(),
        });
        let mut object = PdfPageTextObject::new(&document, text, font, font_size)?;
        object.translate(x, y)?;
        self.add_text_object(object)
    }
    fn create_path_object_line(
        &mut self,
        x1: PdfPoints,
        y1: PdfPoints,
        x2: PdfPoints,
        y2: PdfPoints,
        stroke_color: PdfColor,
        stroke_width: PdfPoints,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let document = PdfDocument::from_inner(backend::PdfDocument {
            inner: self.collection().document(),
        });
        let object =
            PdfPagePathObject::new_line(&document, x1, y1, x2, y2, stroke_color, stroke_width)?;
        self.add_path_object(object)
    }
    fn create_path_object_bezier(
        &mut self,
        x1: PdfPoints,
        y1: PdfPoints,
        x2: PdfPoints,
        y2: PdfPoints,
        control1_x: PdfPoints,
        control1_y: PdfPoints,
        control2_x: PdfPoints,
        control2_y: PdfPoints,
        stroke_color: PdfColor,
        stroke_width: PdfPoints,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let document = PdfDocument::from_inner(backend::PdfDocument {
            inner: self.collection().document(),
        });
        let object = PdfPagePathObject::new_bezier(
            &document,
            x1,
            y1,
            x2,
            y2,
            control1_x,
            control1_y,
            control2_x,
            control2_y,
            stroke_color,
            stroke_width,
        )?;
        self.add_path_object(object)
    }
    fn create_path_object_rect(
        &mut self,
        rect: PdfRect,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let document = PdfDocument::from_inner(backend::PdfDocument {
            inner: self.collection().document(),
        });
        let object =
            PdfPagePathObject::new_rect(&document, rect, stroke_color, stroke_width, fill_color)?;
        self.add_path_object(object)
    }
    fn create_path_object_circle(
        &mut self,
        rect: PdfRect,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let document = PdfDocument::from_inner(backend::PdfDocument {
            inner: self.collection().document(),
        });
        let object =
            PdfPagePathObject::new_circle(&document, rect, stroke_color, stroke_width, fill_color)?;
        self.add_path_object(object)
    }
    fn create_path_object_circle_at(
        &mut self,
        center_x: PdfPoints,
        center_y: PdfPoints,
        radius: PdfPoints,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let document = PdfDocument::from_inner(backend::PdfDocument {
            inner: self.collection().document(),
        });
        let object = PdfPagePathObject::new_circle_at(
            &document,
            center_x,
            center_y,
            radius,
            stroke_color,
            stroke_width,
            fill_color,
        )?;
        self.add_path_object(object)
    }
    fn create_path_object_ellipse(
        &mut self,
        rect: PdfRect,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let document = PdfDocument::from_inner(backend::PdfDocument {
            inner: self.collection().document(),
        });
        let object = PdfPagePathObject::new_ellipse(
            &document,
            rect,
            stroke_color,
            stroke_width,
            fill_color,
        )?;
        self.add_path_object(object)
    }
    fn create_path_object_ellipse_at(
        &mut self,
        center_x: PdfPoints,
        center_y: PdfPoints,
        x_radius: PdfPoints,
        y_radius: PdfPoints,
        stroke_color: Option<PdfColor>,
        stroke_width: Option<PdfPoints>,
        fill_color: Option<PdfColor>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let document = PdfDocument::from_inner(backend::PdfDocument {
            inner: self.collection().document(),
        });
        let object = PdfPagePathObject::new_ellipse_at(
            &document,
            center_x,
            center_y,
            x_radius,
            y_radius,
            stroke_color,
            stroke_width,
            fill_color,
        )?;
        self.add_path_object(object)
    }
    #[cfg(feature = "image_api")]
    fn create_image_object(
        &mut self,
        x: PdfPoints,
        y: PdfPoints,
        image: &DynamicImage,
        width: Option<PdfPoints>,
        height: Option<PdfPoints>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let document = PdfDocument::from_inner(backend::PdfDocument {
            inner: self.collection().document(),
        });
        let mut object = PdfPageImageObject::new(&document)?;
        object.set_image(image)?;
        match (width, height) {
            (Some(w), Some(h)) => {
                object.scale(w.value, h.value)?;
            }
            (Some(w), None) => {
                object.scale(
                    w.value,
                    w.value * image.height() as f32 / image.width() as f32,
                )?;
            }
            (None, Some(h)) => {
                object.scale(
                    h.value * image.width() as f32 / image.height() as f32,
                    h.value,
                )?;
            }
            _ => {}
        }
        object.translate(x, y)?;
        self.add_image_object(object)
    }
    fn remove_object(
        &mut self,
        object: PdfPageObject<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        self.collection().remove(object.handle())?;
        Ok(object)
    }
    fn remove_object_at_index(
        &mut self,
        index: PdfPageObjectIndex,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let object = self.get(index)?;
        self.remove_object(object)
    }
}
impl<'a> PdfPageObjects<'a> {
    pub fn create_group<F>(&'a self, predicate: F) -> Result<PdfPageGroupObject<'a>, PdfiumError>
    where
        F: Fn(&PdfPageObject) -> bool,
    {
        let mut group = self.create_empty_group();
        for mut object in self.iter().filter(predicate) {
            group.push(&mut object)?;
        }
        Ok(group)
    }
    pub fn create_empty_group(&self) -> PdfPageGroupObject<'a> {
        let ObjectCollection::Page(page) = &self.collection else {
            unreachable!()
        };
        PdfPageGroupObject::from_page(page.alias())
    }
    pub fn copy_into_x_object_form_object(
        &self,
        destination: &mut PdfDocument<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let ObjectCollection::Page(page) = &self.collection else {
            unreachable!()
        };
        page.check_live()?;
        let context =
            pdfoxium_core::fpdfapi::edit::cpdf_npagetooneexporter::CpdfNPageToOneExporter::new(
                destination.inner.inner.core.clone(),
                page.document.core.clone(),
            )
            .create_xobject_context_from_page(page.index())
            .ok_or(PdfiumError::PageObjectNotCopyable)?;
        let form = pdfoxium_core::fpdfsdk::fpdf_ppo::new_form_object_from_xobject(&context);
        PdfPageObject::from_handle(ObjectHandle::detached(
            destination.inner.inner.clone(),
            Box::new(form),
        ))
    }
    #[allow(unused_mut)] // Preserve the copied upstream parameter binding.
    pub fn insert_object_at_index(
        &mut self,
        index: PdfPageObjectIndex,
        mut object: PdfPageObject<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        if index >= self.len() {
            return Err(PdfiumError::PageObjectIndexOutOfBounds);
        }
        self.collection.add(object.handle(), Some(index))?;
        Ok(object)
    }
}
impl<'a> PdfPageXObjectFormObject<'a> {
    pub fn len(&self) -> PdfPageObjectIndex {
        self.handle
            .read(|o| {
                Ok(o.as_any()
                    .downcast_ref::<CpdfFormObject>()
                    .ok_or(PdfiumError::UnsupportedPdfPageObjectType)?
                    .form()
                    .holder()
                    .page_object_count())
            })
            .unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfPageObjectIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfPageObjectIndex> {
        0..=self.len().saturating_sub(1)
    }
    pub fn get(&self, index: PdfPageObjectIndex) -> Result<PdfPageObject<'a>, PdfiumError> {
        PdfPageObject::from_handle(ObjectHandle::form(self.handle.clone(), index)?)
    }
    pub fn first(&self) -> Result<PdfPageObject<'a>, PdfiumError> {
        if self.is_empty() {
            Err(PdfiumError::NoPageObjectsInCollection)
        } else {
            self.get(0)
        }
    }
    pub fn last(&self) -> Result<PdfPageObject<'a>, PdfiumError> {
        if self.is_empty() {
            Err(PdfiumError::NoPageObjectsInCollection)
        } else {
            self.get(self.len() - 1)
        }
    }
    pub fn iter(&'a self) -> PdfPageObjectsIterator<'a> {
        PdfPageObjectsIterator::from_form(self)
    }
}

impl private::CollectionOwner for PdfPageXObjectFormObject<'_> {
    fn collection(&self) -> &ObjectCollection {
        &self.collection
    }
}
