// Copyright 2026 The PDFium Authors. BSD-3-Clause; see LICENSE.pdfium.
//! Owned core object references. No C handles, raw pointers or FFI are used.
use crate::{
    backend::{document::DocumentState, PdfPage as CorePage},
    PdfiumError,
};
use pdfoxium_core::{
    fpdfapi::{
        page::{
            cpdf_annotcontext::CpdfAnnotContext,
            cpdf_formobject::CpdfFormObject,
            cpdf_pageobject::PageObject,
            cpdf_pageobjectholder::{CpdfPageObjectHolder, PageObjectId},
        },
        parser::cpdf_object::ObjectRef,
    },
    fpdfsdk::{fpdf_annot, fpdf_editpage},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[derive(Clone)]
pub(crate) struct ObjectHandle(Rc<RefCell<Location>>);
pub(crate) enum Location {
    Detached {
        document: Rc<DocumentState>,
        object: Box<dyn PageObject>,
    },
    Page {
        page: CorePage,
        id: PageObjectId,
        index: Cell<usize>,
        epoch: u64,
    },
    Form {
        parent: ObjectHandle,
        id: PageObjectId,
        index: Cell<usize>,
    },
    Annotation {
        page: CorePage,
        dict: ObjectRef,
        context: Rc<RefCell<CpdfAnnotContext>>,
        id: PageObjectId,
        index: Cell<usize>,
        epoch: u64,
        appearance_epoch: u64,
        state: Rc<crate::annotations::AnnotationState>,
    },
    Removed {
        document: Rc<DocumentState>,
    },
}
fn index(
    holder: &CpdfPageObjectHolder,
    id: PageObjectId,
    hint: &Cell<usize>,
) -> Result<usize, PdfiumError> {
    let index = hint.get();
    if holder.page_object_id_by_index(index) == Some(id) {
        return Ok(index);
    }
    let index = (0..holder.page_object_count())
        .find(|&i| holder.page_object_id_by_index(i) == Some(id))
        .ok_or(PdfiumError::OwnershipNotAttachedToPage)?;
    hint.set(index);
    Ok(index)
}
impl ObjectHandle {
    pub(crate) fn detached(document: Rc<DocumentState>, object: Box<dyn PageObject>) -> Self {
        Self(Rc::new(RefCell::new(Location::Detached {
            document,
            object,
        })))
    }
    pub(crate) fn page(page: &CorePage, position: usize) -> Result<Self, PdfiumError> {
        page.check_live()?;
        let id = page
            .state
            .core
            .borrow()
            .holder()
            .page_object_id_by_index(position)
            .ok_or(PdfiumError::PageObjectIndexOutOfBounds)?;
        Self::cached(
            page.document.clone(),
            id,
            page.state.epoch.get(),
            Location::Page {
                page: page.alias(),
                id,
                index: Cell::new(position),
                epoch: page.state.epoch.get(),
            },
        )
    }
    pub(crate) fn form(parent: ObjectHandle, position: usize) -> Result<Self, PdfiumError> {
        let id = parent.read(|o| {
            o.as_any()
                .downcast_ref::<CpdfFormObject>()
                .and_then(|o| o.form().holder().page_object_id_by_index(position))
                .ok_or(PdfiumError::PageObjectIndexOutOfBounds)
        })?;
        let epoch = parent.owner_page().map_or(0, |p| p.state.epoch.get());
        Self::cached(
            parent.document(),
            id,
            epoch,
            Location::Form {
                parent,
                id,
                index: Cell::new(position),
            },
        )
    }
    pub(crate) fn annotation(
        page: CorePage,
        dict: ObjectRef,
        context: Rc<RefCell<CpdfAnnotContext>>,
        position: usize,
    ) -> Result<Self, PdfiumError> {
        let id = context
            .borrow()
            .form()
            .and_then(|f| f.holder().page_object_id_by_index(position))
            .ok_or(PdfiumError::PageObjectIndexOutOfBounds)?;
        let epoch = page.state.epoch.get();
        let state = annotation_state(&page, &dict)?;
        let appearance_epoch = state.appearance_epoch.get();
        Self::cached(
            page.document.clone(),
            id,
            epoch,
            Location::Annotation {
                page,
                dict,
                context,
                id,
                index: Cell::new(position),
                epoch,
                appearance_epoch,
                state,
            },
        )
    }
    pub(crate) fn document(&self) -> Rc<DocumentState> {
        match &*self.0.borrow() {
            Location::Detached { document, .. } | Location::Removed { document } => {
                document.clone()
            }
            Location::Page { page, .. } | Location::Annotation { page, .. } => {
                page.document.clone()
            }
            Location::Form { parent, .. } => parent.document(),
        }
    }
    pub(crate) fn owner_page(&self) -> Option<CorePage> {
        match &*self.0.borrow() {
            Location::Page { page, .. } | Location::Annotation { page, .. } => Some(page.alias()),
            Location::Form { parent, .. } => parent.owner_page(),
            _ => None,
        }
    }
    pub(crate) fn core_id(&self) -> Option<PageObjectId> {
        match &*self.0.borrow() {
            Location::Page { id, .. }
            | Location::Form { id, .. }
            | Location::Annotation { id, .. } => Some(*id),
            _ => None,
        }
    }
    pub(crate) fn read<T>(
        &self,
        f: impl FnOnce(&dyn PageObject) -> Result<T, PdfiumError>,
    ) -> Result<T, PdfiumError> {
        let mut f = Some(f);
        self.read_dyn(&mut |object| f.take().expect("core visitor called once")(object))
    }
    fn read_dyn<T>(
        &self,
        f: &mut dyn FnMut(&dyn PageObject) -> Result<T, PdfiumError>,
    ) -> Result<T, PdfiumError> {
        let guard = self
            .0
            .try_borrow()
            .map_err(|_| PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        match &*guard {
            Location::Detached { object, .. } => f(object.as_ref()),
            Location::Page {
                page,
                id,
                index: hint,
                epoch,
            } => {
                check_epoch(page, *epoch)?;
                let core = page.state.core.borrow();
                let h = core.holder();
                f(h.page_object_by_index(index(h, *id, hint)?)
                    .ok_or(PdfiumError::OwnershipNotAttachedToPage)?)
            }
            Location::Form {
                parent,
                id,
                index: hint,
            } => parent.read_dyn(&mut |o| {
                let o = o
                    .as_any()
                    .downcast_ref::<CpdfFormObject>()
                    .ok_or(PdfiumError::UnsupportedPdfPageObjectType)?;
                let h = o.form().holder();
                f(h.page_object_by_index(index(h, *id, hint)?)
                    .ok_or(PdfiumError::OwnershipNotAttachedToPage)?)
            }),
            Location::Annotation {
                page,
                dict,
                context,
                id,
                index: hint,
                epoch,
                appearance_epoch,
                state,
            } => {
                if state.appearance_epoch.get() != *appearance_epoch {
                    return Err(PdfiumError::OwnershipNotAttachedToPage);
                }
                check_annotation(page, dict, *epoch)?;
                let context = context.borrow();
                let h = context
                    .form()
                    .ok_or(PdfiumError::OwnershipNotAttachedToPage)?
                    .holder();
                f(h.page_object_by_index(index(h, *id, hint)?)
                    .ok_or(PdfiumError::OwnershipNotAttachedToPage)?)
            }
            Location::Removed { .. } => Err(PdfiumError::OwnershipNotAttachedToPage),
        }
    }
    pub(crate) fn write<T>(
        &self,
        f: impl FnOnce(&mut dyn PageObject) -> Result<T, PdfiumError>,
    ) -> Result<T, PdfiumError> {
        let mut f = Some(f);
        self.write_dyn(&mut |object| f.take().expect("core visitor called once")(object))
    }
    fn write_dyn<T>(
        &self,
        f: &mut dyn FnMut(&mut dyn PageObject) -> Result<T, PdfiumError>,
    ) -> Result<T, PdfiumError> {
        let mut guard = self
            .0
            .try_borrow_mut()
            .map_err(|_| PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let mut edit = |object: &mut dyn PageObject| {
            let result = f(object);
            object.base_mut().set_dirty(true);
            fpdf_editpage::calc_bounding_box(object);
            result
        };
        match &mut *guard {
            Location::Detached { object, .. } => edit(object.as_mut()),
            Location::Page {
                page,
                id,
                index: hint,
                epoch,
            } => {
                check_epoch(page, *epoch)?;
                page.edit(|p| {
                    let h = p.mutable_holder();
                    let position = index(h, *id, hint)?;
                    edit(
                        h.page_object_by_index_mut(position)
                            .ok_or(PdfiumError::OwnershipNotAttachedToPage)?,
                    )
                })
            }
            Location::Form {
                parent,
                id,
                index: hint,
            } => {
                let doc = parent.document();
                parent.write_dyn(&mut |o| {
                    let o = o
                        .as_any_mut()
                        .downcast_mut::<CpdfFormObject>()
                        .ok_or(PdfiumError::UnsupportedPdfPageObjectType)?;
                    o.form_mut()
                        .detach_stream_for_edit(&doc.core.borrow().holder());
                    let h = o.form_mut().holder_mut();
                    let position = index(h, *id, hint)?;
                    edit(
                        h.page_object_by_index_mut(position)
                            .ok_or(PdfiumError::OwnershipNotAttachedToPage)?,
                    )
                })
            }
            Location::Annotation {
                page,
                dict,
                context,
                id,
                index: hint,
                epoch,
                appearance_epoch,
                state,
            } => {
                if state.appearance_epoch.get() != *appearance_epoch {
                    return Err(PdfiumError::OwnershipNotAttachedToPage);
                }
                check_annotation(page, dict, *epoch)?;
                let mut context = context.borrow_mut();
                let h = context
                    .form_mut()
                    .ok_or(PdfiumError::OwnershipNotAttachedToPage)?
                    .holder_mut();
                let position = index(h, *id, hint)?;
                let result = edit(
                    h.page_object_by_index_mut(position)
                        .ok_or(PdfiumError::OwnershipNotAttachedToPage)?,
                );
                if !fpdf_annot::update_content_stream(
                    &mut context,
                    &mut page.document.core.borrow_mut(),
                ) {
                    return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
                }
                result
            }
            Location::Removed { .. } => Err(PdfiumError::OwnershipNotAttachedToPage),
        }
    }
    pub(crate) fn remove(&self) -> Result<(), PdfiumError> {
        let document = self.document();
        let mut guard = self
            .0
            .try_borrow_mut()
            .map_err(|_| PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let object = match &*guard {
            Location::Detached { .. } => return Ok(()),
            Location::Page {
                page, id, epoch, ..
            } => {
                check_epoch(page, *epoch)?;
                page.edit(|p| {
                    p.mutable_holder()
                        .remove_page_object(*id)
                        .ok_or(PdfiumError::OwnershipNotAttachedToPage)
                })?
            }
            Location::Form { .. } => return Err(PdfiumError::PageObjectsCollectionIsImmutable),
            Location::Annotation {
                page,
                dict,
                context,
                id,
                epoch,
                appearance_epoch,
                state,
                ..
            } => {
                if state.appearance_epoch.get() != *appearance_epoch {
                    return Err(PdfiumError::OwnershipNotAttachedToPage);
                }
                check_annotation(page, dict, *epoch)?;
                let mut context = context.borrow_mut();
                let object = context
                    .form_mut()
                    .ok_or(PdfiumError::OwnershipNotAttachedToPage)?
                    .holder_mut()
                    .remove_page_object(*id)
                    .ok_or(PdfiumError::OwnershipNotAttachedToPage)?;
                if !fpdf_annot::update_content_stream(
                    &mut context,
                    &mut page.document.core.borrow_mut(),
                ) {
                    return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
                }
                object
            }
            Location::Removed { .. } => return Err(PdfiumError::OwnershipNotAttachedToPage),
        };
        *guard = Location::Detached { document, object };
        Ok(())
    }
    pub(crate) fn attach_to_page(
        &self,
        page: &CorePage,
        position: Option<usize>,
    ) -> Result<(), PdfiumError> {
        if !Rc::ptr_eq(&self.document(), &page.document) {
            return Err(PdfiumError::CannotMoveObjectAcrossDocuments);
        }
        page.check_live()?;
        self.remove()?;
        let document = self.document();
        let mut guard = self
            .0
            .try_borrow_mut()
            .map_err(|_| PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let old = std::mem::replace(
            &mut *guard,
            Location::Removed {
                document: document.clone(),
            },
        );
        let Location::Detached { mut object, .. } = old else {
            return Err(PdfiumError::OwnershipNotAttachedToPage);
        };
        object.base_mut().set_content_stream(-1);
        object.base_mut().set_dirty(true);
        let (id, position) = page.edit(|p| {
            let h = p.mutable_holder();
            let position = position
                .unwrap_or(h.page_object_count())
                .min(h.page_object_count());
            if !h.insert_page_object_at_index(position, object) {
                return Err(PdfiumError::PageObjectIndexOutOfBounds);
            }
            Ok((
                h.page_object_id_by_index(position)
                    .ok_or(PdfiumError::OwnershipNotAttachedToPage)?,
                position,
            ))
        })?;
        *guard = Location::Page {
            page: page.alias(),
            id,
            index: Cell::new(position),
            epoch: page.state.epoch.get(),
        };
        page.document
            .objects
            .borrow_mut()
            .insert((id, page.state.epoch.get(), 0), Rc::downgrade(&self.0));
        Ok(())
    }
}

impl ObjectHandle {
    pub(crate) fn copy_to(&self, destination: Rc<DocumentState>) -> Result<Self, PdfiumError> {
        Self::copy_group_to(&[self.clone()], destination)
    }
    pub(crate) fn copy_group_to(
        handles: &[Self],
        destination: Rc<DocumentState>,
    ) -> Result<Self, PdfiumError> {
        if handles.is_empty() {
            return Self::empty_group(destination);
        }
        let source = handles[0].document();
        if handles.iter().any(|h| !Rc::ptr_eq(&h.document(), &source)) {
            return Err(PdfiumError::CannotMoveObjectAcrossDocuments);
        }
        use pdfoxium_core::{
            fpdfapi::{
                edit::cpdf_pagecontentgenerator::CpdfPageContentGenerator,
                page::{
                    cpdf_form::CpdfForm, cpdf_imageobject::CpdfImageObject,
                    cpdf_pathobject::CpdfPathObject, cpdf_textobject::CpdfTextObject,
                },
                parser::{cpdf_dictionary::CpdfDictionary, cpdf_stream::CpdfStream},
            },
            fpdfsdk::{fpdf_editimg, fpdf_ppo},
            fxcrt::fx_coordinates::CfxMatrix,
        };
        // Serialize only this object's graphics and resources into an isolated form.
        // This also supports annotation and nested-form objects without changing owners.
        let mut copies = Vec::with_capacity(handles.len());
        for handle in handles {
            let mut object: Box<dyn PageObject> = handle.read(|o| {
                let result: Box<dyn PageObject> =
                    if let Some(text) = o.as_any().downcast_ref::<CpdfTextObject>() {
                        Box::new(text.clone())
                    } else if let Some(path) = o.as_any().downcast_ref::<CpdfPathObject>() {
                        let mut copy = CpdfPathObject::default();
                        *copy.mutable_path() = path.path().clone();
                        copy.set_filltype(path.filltype());
                        copy.set_stroke(path.stroke());
                        copy.set_path_matrix(path.matrix());
                        copy.base_mut().copy_data(path.base());
                        Box::new(copy)
                    } else if let Some(image) = o.as_any().downcast_ref::<CpdfImageObject>() {
                        let mut copy =
                            fpdf_editimg::new_image_object(&source.core.borrow().page_data_ref());
                        copy.set_image(image.image());
                        copy.set_image_matrix(image.matrix());
                        copy.base_mut().copy_data(image.base());
                        Box::new(copy)
                    } else if let Some(form) = o.as_any().downcast_ref::<CpdfFormObject>() {
                        let data = source.core.borrow().page_data_ref();
                        let mut copy = CpdfForm::new(
                            form.form().stream().clone(),
                            form.form().holder().page_resources().cloned(),
                            form.form().holder().resources().cloned(),
                        )
                        .ok_or(PdfiumError::PageObjectNotCopyable)?;
                        copy.parse_content(
                            None,
                            None,
                            data,
                            Rc::new(RefCell::new(Default::default())),
                            0,
                        );
                        let mut copy = CpdfFormObject::new(-1, copy, form.form_matrix());
                        copy.base_mut().copy_data(form.base());
                        Box::new(copy)
                    } else {
                        return Err(PdfiumError::PageObjectNotCopyable);
                    };
                Ok(result)
            })?;
            object.base_mut().set_content_stream(-1);
            object.base_mut().set_dirty(true);
            copies.push(object);
        }
        let content_bbox = copies.iter().fold(
            pdfoxium_core::fxcrt::fx_coordinates::CfxFloatRect::default(),
            |mut rect, o| {
                rect.union(o.base().rect());
                rect
            },
        );
        let bbox = handles[0].owner_page().map_or(content_bbox, |page| {
            pdfoxium_core::fxcrt::fx_coordinates::CfxFloatRect::new(
                0.,
                0.,
                page.width().value,
                page.height().value,
            )
        });
        let holder = source.core.borrow().holder();
        let stream = holder.borrow_mut().new_indirect_stream_from_bytes(&[]);
        struct TemporaryStream {
            holder: pdfoxium_core::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef,
            number: u32,
        }
        impl Drop for TemporaryStream {
            fn drop(&mut self) {
                self.holder.borrow_mut().delete_indirect_object(self.number);
            }
        }
        let _temporary = TemporaryStream {
            holder: holder.clone(),
            number: stream.borrow().obj_num(),
        };
        let dict = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .ok_or(PdfiumError::PageObjectNotCopyable)?
            .get_dict();
        {
            let mut d = dict.borrow_mut();
            let d = d
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .ok_or(PdfiumError::PageObjectNotCopyable)?;
            d.set_new_name_for(b"Type", b"XObject");
            d.set_new_name_for(b"Subtype", b"Form");
            d.set_rect_for(b"BBox", bbox);
            d.set_matrix_for(b"Matrix", CfxMatrix::default());
            d.set_new_for(b"Resources", CpdfDictionary::new());
        }
        let resources = dict
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .and_then(|d| d.get_dict_for(b"Resources"));
        let mut objects = CpdfPageObjectHolder::new(dict, None, resources);
        for object in copies {
            objects.append_page_object(object);
        }
        let mut bytes = Vec::new();
        if !CpdfPageContentGenerator::new(&mut objects, Some(&mut source.core.borrow_mut()))
            .process_page_objects(&mut bytes)
        {
            return Err(PdfiumError::PageObjectNotCopyable);
        }
        stream
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfStream>()
            .ok_or(PdfiumError::PageObjectNotCopyable)?
            .set_data_and_remove_filter(&bytes);
        let stream =
            fpdf_ppo::import_object(destination.core.clone(), source.core.clone(), &stream)
                .ok_or(PdfiumError::PageObjectNotCopyable)?;
        let mut form =
            CpdfForm::new(stream, None, None).ok_or(PdfiumError::PageObjectNotCopyable)?;
        form.parse_content(
            None,
            None,
            destination.core.borrow().page_data_ref(),
            Rc::new(RefCell::new(Default::default())),
            0,
        );
        Ok(Self::detached(
            destination,
            Box::new(CpdfFormObject::new(-1, form, CfxMatrix::default())),
        ))
    }
    pub(crate) fn attach_to_annotation(
        &self,
        page: &CorePage,
        dict: ObjectRef,
        context: Rc<RefCell<CpdfAnnotContext>>,
    ) -> Result<(), PdfiumError> {
        if !Rc::ptr_eq(&self.document(), &page.document) {
            return Err(PdfiumError::CannotMoveObjectAcrossDocuments);
        }
        page.check_live()?;
        let state = annotation_state(page, &dict)?;
        let appearance_epoch = state.appearance_epoch.get();
        self.remove()?;
        let document = self.document();
        let mut guard = self
            .0
            .try_borrow_mut()
            .map_err(|_| PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let old = std::mem::replace(&mut *guard, Location::Removed { document });
        let Location::Detached { mut object, .. } = old else {
            return Err(PdfiumError::OwnershipNotAttachedToPage);
        };
        object.base_mut().set_content_stream(-1);
        object.base_mut().set_dirty(true);
        let (id, position) = {
            let mut context = context.borrow_mut();
            let h = context
                .form_mut()
                .ok_or(PdfiumError::OwnershipNotAttachedToAnnotation)?
                .holder_mut();
            let position = h.page_object_count();
            h.append_page_object(object);
            let id = h
                .page_object_id_by_index(position)
                .ok_or(PdfiumError::OwnershipNotAttachedToPage)?;
            if !fpdf_annot::update_content_stream(
                &mut context,
                &mut page.document.core.borrow_mut(),
            ) {
                return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
            }
            (id, position)
        };
        *guard = Location::Annotation {
            page: page.alias(),
            dict,
            context,
            id,
            index: Cell::new(position),
            epoch: page.state.epoch.get(),
            state,
            appearance_epoch,
        };
        page.document.objects.borrow_mut().insert(
            (id, page.state.epoch.get(), appearance_epoch),
            Rc::downgrade(&self.0),
        );
        Ok(())
    }
}

fn check_epoch(page: &CorePage, epoch: u64) -> Result<(), PdfiumError> {
    page.check_live()?;
    if page.state.epoch.get() != epoch {
        Err(PdfiumError::OwnershipNotAttachedToPage)
    } else {
        Ok(())
    }
}
impl ObjectHandle {
    fn cached(
        document: Rc<DocumentState>,
        id: PageObjectId,
        epoch: u64,
        location: Location,
    ) -> Result<Self, PdfiumError> {
        let appearance_epoch = match &location {
            Location::Annotation {
                appearance_epoch, ..
            } => *appearance_epoch,
            Location::Form { parent, .. } => parent.appearance_epoch(),
            _ => 0,
        };
        let mut cache = document.objects.borrow_mut();
        if let Some(handle) = cache
            .get(&(id, epoch, appearance_epoch))
            .and_then(std::rc::Weak::upgrade)
        {
            return Ok(Self(handle));
        }
        if cache.len() > 1024 {
            cache.retain(|_, v| v.strong_count() > 0);
        }
        let handle = Rc::new(RefCell::new(location));
        cache.insert((id, epoch, appearance_epoch), Rc::downgrade(&handle));
        Ok(Self(handle))
    }
    pub(crate) fn same_object(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ObjectHandle {
    fn empty_group(document: Rc<DocumentState>) -> Result<Self, PdfiumError> {
        use pdfoxium_core::{
            fpdfapi::{
                page::cpdf_form::CpdfForm,
                parser::{cpdf_dictionary::CpdfDictionary, cpdf_stream::CpdfStream},
            },
            fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix},
        };
        let stream = document
            .core
            .borrow()
            .holder()
            .borrow_mut()
            .new_indirect_stream_from_bytes(&[]);
        let dict = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .ok_or(PdfiumError::PageObjectNotCopyable)?
            .get_dict();
        {
            let mut d = dict.borrow_mut();
            let d = d
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .ok_or(PdfiumError::PageObjectNotCopyable)?;
            d.set_new_name_for(b"Type", b"XObject");
            d.set_new_name_for(b"Subtype", b"Form");
            d.set_rect_for(b"BBox", CfxFloatRect::default());
            d.set_new_for(b"Resources", CpdfDictionary::new());
        }
        let form = CpdfForm::new(stream, None, None).ok_or(PdfiumError::PageObjectNotCopyable)?;
        Ok(Self::detached(
            document,
            Box::new(CpdfFormObject::new(-1, form, CfxMatrix::default())),
        ))
    }
}

fn check_annotation(page: &CorePage, dict: &ObjectRef, epoch: u64) -> Result<(), PdfiumError> {
    check_epoch(page, epoch)?;
    annotation_state(page, dict)?.check()
}

fn annotation_state(
    page: &CorePage,
    dict: &ObjectRef,
) -> Result<Rc<crate::annotations::AnnotationState>, PdfiumError> {
    let key = Rc::as_ptr(dict).cast::<()>() as usize;
    page.document
        .annotations
        .borrow()
        .get(&key)
        .and_then(std::rc::Weak::upgrade)
        .ok_or(PdfiumError::OwnershipNotAttachedToPage)
}
impl ObjectHandle {
    fn appearance_epoch(&self) -> u64 {
        match &*self.0.borrow() {
            Location::Annotation {
                appearance_epoch, ..
            } => *appearance_epoch,
            Location::Form { parent, .. } => parent.appearance_epoch(),
            _ => 0,
        }
    }
}
