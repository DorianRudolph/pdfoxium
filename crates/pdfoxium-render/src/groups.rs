// Interfaces copied from pdfium-render 0.9.4; MIT.
use crate::{backend, create_transform_setters, object_store::ObjectHandle, prelude::*};
use std::{marker::PhantomData, rc::Rc};
pub struct PdfPageGroupObject<'a> {
    page: backend::PdfPage,
    object_handles: Vec<ObjectHandle>,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPageGroupObject<'a> {
    pub(crate) fn from_page(page: backend::PdfPage) -> Self {
        Self {
            page,
            object_handles: Vec::new(),
            lifetime: PhantomData,
        }
    }
    fn apply_to_each<F, T>(&mut self, mut f: F) -> Result<(), PdfiumError>
    where
        F: FnMut(&mut PdfPageObject<'a>) -> Result<T, PdfiumError>,
    {
        let _batch = Batch::new(&self.page);
        let mut error = None;
        for h in &self.object_handles {
            match PdfPageObject::from_handle(h.clone()).and_then(|mut o| f(&mut o)) {
                Ok(_) => {}
                Err(e) => error = Some(e),
            }
        }
        error.map_or(Ok(()), Err)
    }
    fn for_each<F: FnMut(&mut PdfPageObject<'a>)>(&self, mut f: F) {
        for h in &self.object_handles {
            if let Ok(mut o) = PdfPageObject::from_handle(h.clone()) {
                f(&mut o);
            }
        }
    }
    create_transform_setters!(&mut Self,Result<(),PdfiumError>,"every object in this group","every object in this group.","every object in this group,");
    fn transform_impl(
        &mut self,
        a: PdfMatrixValue,
        b: PdfMatrixValue,
        c: PdfMatrixValue,
        d: PdfMatrixValue,
        e: PdfMatrixValue,
        f: PdfMatrixValue,
    ) -> Result<(), PdfiumError> {
        self.apply_to_each(|o| o.transform(a, b, c, d, e, f))
    }
    fn reset_matrix_impl(&mut self, matrix: PdfMatrix) -> Result<(), PdfiumError> {
        self.apply_to_each(|o| o.reset_matrix(matrix))
    }
}
struct Batch {
    page: backend::PdfPage,
    strategy: PdfPageContentRegenerationStrategy,
}
impl Batch {
    fn new(page: &backend::PdfPage) -> Self {
        let strategy = page
            .state
            .strategy
            .replace(PdfPageContentRegenerationStrategy::Manual);
        Self {
            page: page.alias(),
            strategy,
        }
    }
}
impl Drop for Batch {
    fn drop(&mut self) {
        self.page.state.strategy.set(self.strategy);
        if self.strategy == PdfPageContentRegenerationStrategy::AutomaticOnEveryChange {
            let _ = self.page.regenerate_content();
        }
    }
}
impl<'a> PdfPageGroupObject<'a> {
    pub fn empty(page: &'a PdfPage) -> Self {
        Self::from_page(page.inner.alias())
    }
    pub fn new<F>(page: &'a PdfPage, predicate: F) -> Result<Self, PdfiumError>
    where
        F: FnMut(&PdfPageObject) -> bool,
    {
        let mut result = Self::from_page(page.inner.alias());
        for mut object in page.objects().iter().filter(predicate) {
            result.push(&mut object)?;
        }
        Ok(result)
    }
    pub fn from_vec(
        page: &PdfPage<'a>,
        mut objects: Vec<PdfPageObject<'a>>,
    ) -> Result<Self, PdfiumError> {
        Self::from_slice(page, objects.as_mut_slice())
    }
    pub fn from_slice(
        page: &PdfPage<'a>,
        objects: &mut [PdfPageObject<'a>],
    ) -> Result<Self, PdfiumError> {
        let mut result = Self::from_page(page.inner.alias());

        for object in objects.iter_mut() {
            result.push(object)?;
        }

        Ok(result)
    }
    pub fn len(&self) -> usize {
        self.object_handles.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn contains(&self, object: &PdfPageObject) -> bool {
        self.object_handles
            .iter()
            .any(|h| h.same_object(object.handle()))
    }
    pub fn push(&mut self, object: &mut PdfPageObject<'a>) -> Result<(), PdfiumError> {
        if let Some(page) = object.handle().owner_page() {
            if !Rc::ptr_eq(&page.state, &self.page.state) {
                return Err(PdfiumError::OwnershipAlreadyAttachedToDifferentPage);
            }
        } else {
            object.handle().attach_to_page(&self.page, None)?;
        }
        if !self.contains(object) {
            self.object_handles.push(object.handle().clone());
        }
        Ok(())
    }
    pub fn append(&mut self, objects: &mut [PdfPageObject<'a>]) -> Result<(), PdfiumError> {
        let _batch = Batch::new(&self.page);
        for object in objects.iter_mut() {
            self.push(object)?;
        }
        Ok(())
    }
    pub fn remove_objects_from_page(mut self) -> Result<(), PdfiumError> {
        let _batch = Batch::new(&self.page);
        for handle in &self.object_handles {
            handle.remove()?;
        }
        self.object_handles.clear();
        Ok(())
    }
    pub fn get(&self, index: PdfPageObjectIndex) -> Result<PdfPageObject<'_>, PdfiumError> {
        PdfPageObject::from_handle(
            self.object_handles
                .get(index)
                .ok_or(PdfiumError::PageObjectIndexOutOfBounds)?
                .clone(),
        )
    }
    pub fn retain<F>(&mut self, f: F)
    where
        F: Fn(&PdfPageObject) -> bool,
    {
        let retain = self
            .object_handles
            .iter()
            .map(|h| PdfPageObject::from_handle(h.clone()).is_ok_and(|o| f(&o)))
            .collect::<Vec<_>>();
        let mut i = 0;
        self.object_handles.retain(|_| {
            let keep = retain[i];
            i += 1;
            keep
        })
    }
    pub fn move_to_page(mut self, page: &mut PdfPage) -> Result<(), PdfiumError> {
        for handle in &self.object_handles {
            handle.attach_to_page(&page.inner, None)?;
        }
        self.object_handles.clear();
        Ok(())
    }
    pub fn move_to_annotation(
        mut self,
        annotation: &mut PdfPageAnnotation,
    ) -> Result<(), PdfiumError> {
        let (page, dict, context) = annotation.core_object_target()?;
        for handle in &self.object_handles {
            handle.attach_to_annotation(&page, dict.clone(), context.clone())?;
        }
        self.object_handles.clear();
        Ok(())
    }
    pub fn copy_to_page(
        &mut self,
        page: &mut PdfPage<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        let object = PdfPageObject::from_handle(ObjectHandle::copy_group_to(
            &self.object_handles,
            page.inner.document.clone(),
        )?)?;
        page.objects_mut().add_object(object)
    }
    pub fn copy_into_x_object_form_object(
        &mut self,
        destination: &mut PdfDocument<'a>,
    ) -> Result<PdfPageObject<'a>, PdfiumError> {
        PdfPageObject::from_handle(ObjectHandle::copy_group_to(
            &self.object_handles,
            destination.inner.inner.clone(),
        )?)
    }
    pub fn iter(&'a self) -> PdfPageGroupObjectIterator<'a> {
        PdfPageGroupObjectIterator {
            group: self,
            next_index: 0,
        }
    }
    pub fn text(&self) -> String {
        self.text_separated("")
    }
    pub fn text_separated(&self, separator: &str) -> String {
        let mut strings = Vec::with_capacity(self.len());

        self.for_each(|object| {
            if let Some(object) = object.as_text_object() {
                strings.push(object.text());
            }
        });

        strings.join(separator)
    }
    pub fn has_transparency(&self) -> bool {
        self.object_handles
            .iter()
            .any(|h| PdfPageObject::from_handle(h.clone()).is_ok_and(|o| o.has_transparency()))
    }
    pub fn bounds(&self) -> Result<PdfRect, PdfiumError> {
        let mut bottom = PdfPoints::MAX;
        let mut left = PdfPoints::MAX;
        let mut top = PdfPoints::MIN;
        let mut right = PdfPoints::MIN;
        let mut empty = true;
        for h in &self.object_handles {
            if let Ok(bounds) = PdfPageObject::from_handle(h.clone()).and_then(|o| o.bounds()) {
                bottom = bottom.min(bounds.bottom());
                left = left.min(bounds.left());
                top = top.max(bounds.top());
                right = right.max(bounds.right());
                empty = false;
            }
        }
        if empty {
            Err(PdfiumError::EmptyPageObjectGroup)
        } else {
            Ok(PdfRect::new(bottom, left, top, right))
        }
    }
    pub fn set_blend_mode(
        &mut self,
        blend_mode: PdfPageObjectBlendMode,
    ) -> Result<(), PdfiumError> {
        self.apply_to_each(|object| object.set_blend_mode(blend_mode))
    }
    pub fn set_fill_color(&mut self, fill_color: PdfColor) -> Result<(), PdfiumError> {
        self.apply_to_each(|object| object.set_fill_color(fill_color))
    }
    pub fn set_stroke_color(&mut self, stroke_color: PdfColor) -> Result<(), PdfiumError> {
        self.apply_to_each(|object| object.set_stroke_color(stroke_color))
    }
    pub fn set_stroke_width(&mut self, stroke_width: PdfPoints) -> Result<(), PdfiumError> {
        self.apply_to_each(|object| object.set_stroke_width(stroke_width))
    }
    pub fn set_line_join(&mut self, line_join: PdfPageObjectLineJoin) -> Result<(), PdfiumError> {
        self.apply_to_each(|object| object.set_line_join(line_join))
    }
    pub fn set_line_cap(&mut self, line_cap: PdfPageObjectLineCap) -> Result<(), PdfiumError> {
        self.apply_to_each(|object| object.set_line_cap(line_cap))
    }
    pub fn set_fill_and_stroke_mode(
        &mut self,
        fill_mode: PdfPathFillMode,
        do_stroke: bool,
    ) -> Result<(), PdfiumError> {
        self.apply_to_each(|object| {
            if let Some(object) = object.as_path_object_mut() {
                object.set_fill_and_stroke_mode(fill_mode, do_stroke)
            } else {
                Ok(())
            }
        })
    }
}
pub struct PdfPageGroupObjectIterator<'a> {
    group: &'a PdfPageGroupObject<'a>,
    next_index: PdfPageObjectIndex,
}
impl<'a> Iterator for PdfPageGroupObjectIterator<'a> {
    type Item = PdfPageObject<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.group.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.group.get(i).ok()
    }
}
