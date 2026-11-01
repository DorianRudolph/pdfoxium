// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Color-space resource resolution and caching from
//! `core/fpdfapi/page/cpdf_docpagedata.{h,cpp}`. Color-space, image, and
//! pattern, ICC profile, embedded face and PDF font caches retain document
//! resources. Editing font creation remains deferred.

use sha1::{Digest, Sha1};
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::{Rc, Weak};

use super::cpdf_colorspace::{
    get_stock_cs_for_name, load_color_space_with_context, BaseSpaceLoad, ColorSpaceFamily,
    ColorSpaceRef,
};
use super::cpdf_form::{CpdfForm, CpdfType3Form};
use super::cpdf_iccprofile::CpdfIccProfile;
use super::cpdf_image::CpdfImage;
use super::cpdf_pattern::{CpdfPattern, PatternRef};
use super::cpdf_shadingpattern::CpdfShadingPattern;
use crate::fpdfapi::font::cpdf_cidfont::CpdfLoadedCidFont;
use crate::fpdfapi::font::cpdf_facebasedsimplefont::CpdfLoadedSimpleFont;
use crate::fpdfapi::font::cpdf_font::{CpdfFont, CpdfFontBase};
use crate::fpdfapi::font::cpdf_fontencoding::CpdfFontEncoding;
use crate::fpdfapi::font::cpdf_fontglobals::CpdfFontGlobals;
use crate::fpdfapi::font::cpdf_stockfontarray::CpdfStockFontArray;
use crate::fpdfapi::font::cpdf_type3char::Type3Form;
use crate::fpdfapi::font::cpdf_type3font::{CpdfType3Font, Type3FormFactory};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef, PdfObject};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::{CpdfRetainedStreamAcc, CpdfStreamAcc};
use crate::fpdfapi::render::cpdf_docrenderdata::CpdfDocRenderData;
use crate::fxcodec::jbig2::jbig2_document_context::Jbig2DocumentContext;
use crate::fxge::cfx_face::{CfxFaceError, CfxOwnedFace};
use crate::fxge::cfx_fontmapper::CfxFontMapper;
use crate::fxge::cfx_standardfont::{get_canonical_font_name, get_standard_font_index};

use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fxcrt::fx_codepage::{charset_is_cjk, FxCharset, CHARSET_UNICODES};
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use crate::fxge::cfx_font::CfxFont;
use crate::fxge::fx_font::{
    adobe_name_from_unicode, FONT_STYLE_FIXED_PITCH, FONT_STYLE_FORCE_BOLD, FONT_STYLE_ITALIC,
    FONT_STYLE_NON_SYMBOLIC, FONT_STYLE_SYMBOLIC,
};

type CachedSpace = (ObjectRef, ColorSpaceRef);
type CachedPattern = (ObjectRef, PatternRef);
pub type CachedDocumentFace = CfxOwnedFace<Rc<CachedFontFile>>;

struct DocumentType3FormFactory(Weak<CpdfDocPageData>);

impl Type3FormFactory for DocumentType3FormFactory {
    fn create_form(
        &self,
        resources: Option<ObjectRef>,
        stream: ObjectRef,
    ) -> Option<Box<dyn Type3Form>> {
        self.0.upgrade()?;
        let form = CpdfForm::new(stream, resources, None)?;
        Some(Box::new(CpdfType3Form::new(form, self.0.clone())))
    }
}

pub struct CachedFontFile {
    stream: ObjectRef,
    data: Rc<Vec<u8>>,
}

impl CachedFontFile {
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn stream(&self) -> &ObjectRef {
        &self.stream
    }

    pub fn shared_data(&self) -> Rc<Vec<u8>> {
        Rc::clone(&self.data)
    }
}

impl AsRef<[u8]> for CachedFontFile {
    fn as_ref(&self) -> &[u8] {
        self.data()
    }
}

#[derive(Default)]
pub struct CpdfDocPageData {
    codec_context: OnceCell<Rc<RefCell<Jbig2DocumentContext>>>,
    // The selected uses of the native unowned document backpointer. Pages
    // retain PageData, so these links must not retain the document graph.
    document_objects: RefCell<Weak<RefCell<CpdfIndirectObjectHolder>>>,
    document_root: RefCell<Option<Weak<RefCell<dyn PdfObject>>>>,
    // Shared document bookkeeping permits holder completion without borrowing
    // the document again during nested form parsing.
    parsed_page_count: Cell<u32>,
    render_data: Rc<CpdfDocRenderData>,
    color_space_map: RefCell<HashMap<usize, CachedSpace>>,
    icc_profile_map: RefCell<HashMap<usize, (ObjectRef, Rc<CpdfIccProfile>)>>,
    hash_icc_profile_map: RefCell<BTreeMap<(u32, [u8; 20]), ObjectRef>>,
    image_map: RefCell<HashMap<u32, Rc<CpdfImage>>>,
    pattern_map: RefCell<HashMap<usize, CachedPattern>>,
    font_file_map: RefCell<HashMap<usize, Rc<CachedFontFile>>>,
    font_face_map: RefCell<HashMap<usize, Rc<CachedDocumentFace>>>,
    simple_font_map: RefCell<HashMap<usize, Rc<CpdfLoadedSimpleFont>>>,
    cid_font_map: RefCell<HashMap<usize, Rc<CpdfLoadedCidFont>>>,
    type3_font_map: RefCell<HashMap<usize, (ObjectRef, Rc<CpdfType3Font>)>>,
    font_globals: RefCell<CpdfFontGlobals>,
    stock_fonts: RefCell<CpdfStockFontArray>,
}

impl Drop for CpdfDocPageData {
    fn drop(&mut self) {
        drop(std::mem::take(self.stock_fonts.get_mut()));
        drop(self.codec_context.take());
        for image in self.image_map.get_mut().values() {
            image.will_be_destroyed();
        }
        for (_, font) in self.type3_font_map.get_mut().values() {
            font.will_be_destroyed();
        }
        // Native member destruction runs in reverse declaration order after
        // the WillBeDestroyed hooks. Keep resources alive through font teardown.
        drop(std::mem::take(self.type3_font_map.get_mut()));
        drop(std::mem::take(self.cid_font_map.get_mut()));
        drop(std::mem::take(self.simple_font_map.get_mut()));
        drop(std::mem::take(self.image_map.get_mut()));
        drop(std::mem::take(self.pattern_map.get_mut()));
        drop(std::mem::take(self.icc_profile_map.get_mut()));
        drop(std::mem::take(self.font_face_map.get_mut()));
        drop(std::mem::take(self.font_file_map.get_mut()));
        drop(std::mem::take(self.color_space_map.get_mut()));
        drop(std::mem::take(self.hash_icc_profile_map.get_mut()));
    }
}

impl CpdfDocPageData {
    /// Portable `CPDF_DocPageData::AddFont()`: add a substituted native face's
    /// PDF font dictionary, encoding and metrics to this document.
    pub fn add_font(
        self: &Rc<Self>,
        font: crate::fxge::cfx_font::CfxFont<'_>,
        charset: crate::fxcrt::fx_codepage::FxCharset,
        objects: &HolderRef,
        mapper: &mut CfxFontMapper,
    ) -> Option<CpdfFont> {
        font.face()?;
        let dictionary = add_native_font_dictionary(objects, &font, charset);
        self.get_font(&dictionary, mapper)
    }

    pub fn new() -> Self {
        Self::default()
    }
    pub(crate) fn get_or_create_codec_context(&self) -> Rc<RefCell<Jbig2DocumentContext>> {
        Rc::clone(
            self.codec_context
                .get_or_init(|| Rc::new(RefCell::new(Jbig2DocumentContext::default()))),
        )
    }
    pub(crate) fn bind_document_resources(&self, objects: &HolderRef, root: Option<&ObjectRef>) {
        *self.document_objects.borrow_mut() = Rc::downgrade(objects);
        *self.document_root.borrow_mut() = root.map(Rc::downgrade);
    }
    pub(crate) fn document_owner_identity(
        &self,
    ) -> Option<Weak<RefCell<CpdfIndirectObjectHolder>>> {
        let owner = self.document_objects.borrow().clone();
        // Keep an expired allocation's identity as well: Weak retains the
        // control block, so a later document cannot reuse its identity.
        (!Weak::ptr_eq(&owner, &Weak::new())).then_some(owner)
    }
    pub(crate) fn document_objects(&self) -> Option<HolderRef> {
        self.document_objects.borrow().upgrade()
    }
    pub(crate) fn document_root(&self) -> Option<ObjectRef> {
        self.document_root.borrow().as_ref()?.upgrade()
    }
    pub(crate) fn parsed_page_count(&self) -> u32 {
        self.parsed_page_count.get()
    }
    pub(crate) fn increment_parsed_page_count(&self) {
        self.parsed_page_count
            .set(self.parsed_page_count.get().wrapping_add(1));
    }
    pub fn render_data(&self) -> &CpdfDocRenderData {
        &self.render_data
    }
    pub fn render_data_ref(&self) -> Rc<CpdfDocRenderData> {
        Rc::clone(&self.render_data)
    }

    /// `CPDF_Font::Create()` dispatch with the retained `GetFont()` caches.
    pub fn get_font(
        self: &Rc<Self>,
        font_dict: &ObjectRef,
        mapper: &mut CfxFontMapper,
    ) -> Option<CpdfFont> {
        let borrowed = font_dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        if CpdfFontBase::uses_cid_font(dict) {
            self.get_cid_font(font_dict, mapper).map(CpdfFont::Cid)
        } else if crate::fxcrt::bytestring::eq_cstr(
            &dict.get_byte_string_for(b"Subtype", b""),
            b"Type3",
        ) {
            self.get_type3_font(font_dict).map(CpdfFont::Type3)
        } else {
            self.get_simple_font(font_dict, mapper)
                .map(CpdfFont::Simple)
        }
    }

    pub fn add_standard_font(
        &self,
        name: &[u8],
        encoding: Option<&CpdfFontEncoding>,
        objects: &HolderRef,
        mapper: &mut CfxFontMapper,
    ) -> Option<Rc<CpdfLoadedSimpleFont>> {
        let index = get_standard_font_index(name)?;
        self.get_standard_font(
            get_canonical_font_name(index).as_bytes(),
            encoding,
            objects,
            mapper,
        )
    }

    fn get_standard_font(
        &self,
        name: &[u8],
        encoding: Option<&CpdfFontEncoding>,
        objects: &HolderRef,
        mapper: &mut CfxFontMapper,
    ) -> Option<Rc<CpdfLoadedSimpleFont>> {
        if name.is_empty() {
            return None;
        }
        {
            let fonts = self.simple_font_map.borrow();
            // The native map is ordered by dictionary identity. Select the
            // first eligible identity without a temporary collection.
            let existing = fonts
                .iter()
                .filter(|(_, font)| {
                    if font.base_font_name() != name || font.is_embedded() || !font.is_type1() {
                        return false;
                    }
                    let Some(dict) = font.font_dict() else {
                        return false;
                    };
                    if dict
                        .borrow()
                        .as_any()
                        .downcast_ref::<CpdfDictionary>()
                        .is_none_or(|dict| dict.key_exist(b"Widths"))
                    {
                        return false;
                    }
                    encoding
                        .is_none_or(|encoding| font.base().simple.encoding().is_identical(encoding))
                })
                .min_by_key(|(identity, _)| *identity);
            if let Some((_, font)) = existing {
                return Some(Rc::clone(font));
            }
        }
        let mut dict = CpdfDictionary::with_pool(objects.borrow().byte_string_pool());
        dict.set_new_name_for(b"Type", b"Font");
        dict.set_new_name_for(b"Subtype", b"Type1");
        dict.set_new_name_for(b"BaseFont", name);
        if let Some(encoding) = encoding {
            dict.set_for(
                b"Encoding",
                encoding.realize(objects.borrow().byte_string_pool()),
            );
        }
        let dict = retain_object(dict);
        objects.borrow_mut().add_indirect_object(Rc::clone(&dict));
        self.get_simple_font(&dict, mapper)
    }

    /// `CPDF_Font::GetStockFont()`. Page data owns the document's stock array
    /// directly, replacing the global map keyed by an unowned document pointer.
    pub fn get_stock_font(
        &self,
        name: &[u8],
        mapper: &mut CfxFontMapper,
    ) -> Option<Rc<CpdfLoadedSimpleFont>> {
        let index = get_standard_font_index(name)?;
        if let Some(font) = self.stock_fonts.borrow().get_font(index) {
            let font: Rc<dyn std::any::Any> = font;
            return Rc::downcast::<CpdfLoadedSimpleFont>(font).ok();
        }
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Type", b"Font");
        dict.set_new_name_for(b"Subtype", b"Type1");
        dict.set_new_name_for(b"BaseFont", get_canonical_font_name(index).as_bytes());
        dict.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
        let font = Rc::new(CpdfLoadedSimpleFont::load(
            retain_object(dict),
            self,
            mapper,
        )?);
        self.stock_fonts
            .borrow_mut()
            .set_font(index, Some(font.clone()));
        Some(font)
    }

    /// Face-based branch of `GetFont()`. The loader runs without a cache borrow
    /// so loading dependencies can re-enter page data.
    pub fn get_simple_font(
        &self,
        font_dict: &ObjectRef,
        mapper: &mut CfxFontMapper,
    ) -> Option<Rc<CpdfLoadedSimpleFont>> {
        let identity = Rc::as_ptr(font_dict) as *const () as usize;
        if let Some(font) = self.simple_font_map.borrow().get(&identity) {
            return Some(Rc::clone(font));
        }
        let font = Rc::new(CpdfLoadedSimpleFont::load(
            Rc::clone(font_dict),
            self,
            mapper,
        )?);
        self.simple_font_map
            .borrow_mut()
            .insert(identity, Rc::clone(&font));
        Some(font)
    }

    pub fn get_cid_font(
        &self,
        font_dict: &ObjectRef,
        mapper: &mut CfxFontMapper,
    ) -> Option<Rc<CpdfLoadedCidFont>> {
        let identity = Rc::as_ptr(font_dict) as *const () as usize;
        if let Some(font) = self.cid_font_map.borrow().get(&identity) {
            return Some(Rc::clone(font));
        }
        let font = Rc::new(CpdfLoadedCidFont::load(
            Rc::clone(font_dict),
            self,
            mapper,
            &mut self.font_globals.borrow_mut(),
        )?);
        self.cid_font_map
            .borrow_mut()
            .insert(identity, Rc::clone(&font));
        Some(font)
    }

    pub fn get_type3_font(self: &Rc<Self>, font_dict: &ObjectRef) -> Option<Rc<CpdfType3Font>> {
        let identity = Rc::as_ptr(font_dict) as *const () as usize;
        if let Some((_, font)) = self.type3_font_map.borrow().get(&identity) {
            return Some(Rc::clone(font));
        }
        let mut font = CpdfType3Font::new(Rc::new(DocumentType3FormFactory(Rc::downgrade(self))));
        let borrowed = font_dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        font.load(dict, &dict.get_byte_string_for(b"BaseFont", b""));
        font.set_font_dict(font_dict);
        drop(borrowed);
        let font = Rc::new(font);
        self.type3_font_map
            .borrow_mut()
            .insert(identity, (Rc::clone(font_dict), Rc::clone(&font)));
        Some(font)
    }

    pub fn get_font_file_data(&self, stream: &ObjectRef) -> Option<Rc<CachedFontFile>> {
        let identity = Rc::as_ptr(stream) as *const () as usize;
        if let Some(cached) = self.font_file_map.borrow().get(&identity) {
            return Some(Rc::clone(cached));
        }
        let data = {
            let borrowed = stream.borrow();
            let stream_obj = borrowed.as_any().downcast_ref::<CpdfStream>()?;
            if stream_obj.is_memory_based() && !stream_obj.has_filter() {
                stream_obj.share_in_memory_raw_data()?
            } else {
                let dict = stream_obj.get_dict();
                let borrowed_dict = dict.borrow();
                let descriptor = borrowed_dict.as_any().downcast_ref::<CpdfDictionary>()?;
                let parts = [
                    b"Length1".as_slice(),
                    b"Length2".as_slice(),
                    b"Length3".as_slice(),
                ];
                let lengths = parts.map(|key| descriptor.get_integer_for(key, 0));
                let estimated = lengths
                    .into_iter()
                    .try_fold(0u32, |sum, length| {
                        sum.checked_add(u32::try_from(length).ok()?)
                    })
                    .unwrap_or(0);
                drop(borrowed_dict);
                let mut accessor = CpdfStreamAcc::new(stream_obj);
                accessor.load_all_data_filtered_with_estimated_size(estimated);
                accessor.detach_shared_data()
            }
        };
        let cached = Rc::new(CachedFontFile {
            stream: Rc::clone(stream),
            data,
        });
        self.font_file_map
            .borrow_mut()
            .insert(identity, Rc::clone(&cached));
        Some(cached)
    }

    /// Keep parsed embedded faces with their decoded font file for the document.
    pub fn get_font_file_face(
        &self,
        stream: &ObjectRef,
    ) -> Result<Option<Rc<CachedDocumentFace>>, CfxFaceError> {
        let identity = Rc::as_ptr(stream) as *const () as usize;
        if let Some(face) = self.font_face_map.borrow().get(&identity) {
            return Ok(Some(Rc::clone(face)));
        }
        let Some(bytes) = self.get_font_file_data(stream) else {
            return Ok(None);
        };
        let face = match CfxOwnedFace::new(Rc::clone(&bytes), 0) {
            Ok(face) => Rc::new(face),
            Err(error) => {
                self.maybe_purge_font_file_data(bytes);
                return Err(error);
            }
        };
        self.font_face_map
            .borrow_mut()
            .insert(identity, Rc::clone(&face));
        Ok(Some(face))
    }

    pub fn maybe_purge_font_file_data(&self, cached: Rc<CachedFontFile>) {
        let identity = Rc::as_ptr(&cached.stream) as *const () as usize;
        drop(cached);
        let mut files = self.font_file_map.borrow_mut();
        if files
            .get(&identity)
            .is_some_and(|file| Rc::strong_count(file) == 1)
        {
            files.remove(&identity);
        }
    }

    pub fn get_image(&self, stream: &ObjectRef) -> Option<Rc<CpdfImage>> {
        let obj_num = stream.borrow().obj_num();
        if obj_num == 0 {
            return None;
        }
        if let Some(image) = self.image_map.borrow().get(&obj_num) {
            return Some(Rc::clone(image));
        }
        let image = Rc::new(CpdfImage::from_stream_with_page_data(
            stream.clone(),
            false,
            self,
        )?);
        self.image_map
            .borrow_mut()
            .insert(obj_num, Rc::clone(&image));
        Some(image)
    }
    pub fn maybe_purge_image(&self, obj_num: u32) {
        if obj_num == 0 {
            return;
        }
        let mut images = self.image_map.borrow_mut();
        let stream = images
            .get(&obj_num)
            .filter(|image| Rc::strong_count(image) == 1)
            .map(|image| image.stream().clone());
        if stream.is_some() {
            images.remove(&obj_num);
        }
    }

    pub fn get_pattern(
        &self,
        object: &ObjectRef,
        matrix: crate::fxcrt::fx_coordinates::CfxMatrix,
    ) -> Option<PatternRef> {
        let identity = Rc::as_ptr(object) as *const () as usize;
        if let Some(pattern) = self
            .pattern_map
            .borrow()
            .get(&identity)
            .map(|(_, cached)| Rc::clone(cached))
        {
            return Some(pattern);
        }
        let kind = {
            let dict = crate::fpdfapi::parser::cpdf_object::get_dict_object(object)?;
            let borrowed = dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_integer_for(b"PatternType", 0)
        };
        let pattern = match kind {
            1 => CpdfPattern::new_tiling(object.clone(), matrix)?,
            2 => CpdfPattern::new_shading(object.clone(), false, matrix)?,
            _ => return None,
        };
        let pattern = Rc::new(pattern);
        self.pattern_map
            .borrow_mut()
            .insert(identity, (object.clone(), Rc::clone(&pattern)));
        Some(pattern)
    }

    pub fn get_shading(
        &self,
        object: &ObjectRef,
        matrix: crate::fxcrt::fx_coordinates::CfxMatrix,
    ) -> Option<Rc<CpdfShadingPattern>> {
        let identity = Rc::as_ptr(object) as *const () as usize;
        if let Some(pattern) = self
            .pattern_map
            .borrow()
            .get(&identity)
            .map(|(_, cached)| Rc::clone(cached))
        {
            // AsShadingPattern returns null for an existing tiling entry.
            return CpdfShadingPattern::retained(pattern);
        }
        crate::fpdfapi::parser::cpdf_object::get_dict_object(object)?;
        let pattern = Rc::new(CpdfPattern::new_shading(object.clone(), true, matrix)?);
        self.pattern_map
            .borrow_mut()
            .insert(identity, (object.clone(), Rc::clone(&pattern)));
        CpdfShadingPattern::retained(pattern)
    }

    pub fn get_icc_profile(&self, stream: &ObjectRef) -> Option<Rc<CpdfIccProfile>> {
        let identity = Rc::as_ptr(stream) as *const () as usize;
        if let Some((_, profile)) = self.icc_profile_map.borrow().get(&identity) {
            return Some(Rc::clone(profile));
        }
        let accessor = CpdfRetainedStreamAcc::load_filtered(Rc::clone(stream))?;
        let expected = {
            let borrowed = stream.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfStream>()?.get_dict();
            let borrowed = dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_integer_for(b"N", 0)
        };
        assert!(
            crate::fxcodec::icc::icc_transform::IccTransform::is_valid_icc_components(expected)
        );
        let key = (
            expected as u32,
            <[u8; 20]>::from(Sha1::digest(&*accessor.get_span())),
        );
        if let Some(copied_stream) = self.hash_icc_profile_map.borrow().get(&key) {
            let copied_identity = Rc::as_ptr(copied_stream) as *const () as usize;
            if let Some((_, profile)) = self.icc_profile_map.borrow().get(&copied_identity) {
                // Native hash hits do not insert an entry for the alias stream.
                return Some(Rc::clone(profile));
            }
        }
        let profile = Rc::new(CpdfIccProfile::from_accessor(accessor, expected as u32)?);
        self.icc_profile_map
            .borrow_mut()
            .insert(identity, (Rc::clone(stream), Rc::clone(&profile)));
        self.hash_icc_profile_map
            .borrow_mut()
            .insert(key, Rc::clone(stream));
        Some(profile)
    }

    pub fn get_color_space(
        &self,
        object: &ObjectRef,
        resources: Option<&ObjectRef>,
    ) -> Option<ColorSpaceRef> {
        self.get_color_space_guarded(object, resources, &mut HashSet::new())
    }

    pub fn get_color_space_guarded(
        &self,
        object: &ObjectRef,
        resources: Option<&ObjectRef>,
        visited: &mut HashSet<usize>,
    ) -> Option<ColorSpaceRef> {
        self.get_color_space_internal(object, resources, visited, &mut HashSet::new())
    }

    fn load_color_space_direct(
        &self,
        object: &ObjectRef,
        visited: &mut HashSet<usize>,
    ) -> Option<ColorSpaceRef> {
        let object = get_direct_object(object)?;
        let identity = Rc::as_ptr(&object) as *const () as usize;
        if !visited.insert(identity) {
            return None;
        }
        let loaded = load_color_space_with_context(
            &object,
            &mut |base, mode| match mode {
                BaseSpaceLoad::Direct => self.load_color_space_direct(base, visited),
                BaseSpaceLoad::DocumentCache => self.get_color_space_guarded(base, None, visited),
            },
            &mut |stream, _| self.get_icc_profile(stream),
        );
        visited.remove(&identity);
        loaded
    }

    fn get_color_space_internal(
        &self,
        object: &ObjectRef,
        resources: Option<&ObjectRef>,
        visited: &mut HashSet<usize>,
        visited_internal: &mut HashSet<usize>,
    ) -> Option<ColorSpaceRef> {
        let object = get_direct_object(object)?;
        let identity = Rc::as_ptr(&object) as *const () as usize;
        if !visited_internal.insert(identity) {
            return None;
        }
        let result = (|| {
            let kind = object.borrow().object_type();
            if kind == ObjectType::Name {
                let name = object.borrow().get_string();
                let stock = get_stock_cs_for_name(&name);
                let named_resources = resources.and_then(|resources| {
                    let borrowed = resources.borrow();
                    borrowed
                        .as_any()
                        .downcast_ref::<CpdfDictionary>()?
                        .get_dict_for(b"ColorSpace")
                });
                if stock.is_none() {
                    let named = named_resources.and_then(|color_spaces| {
                        let borrowed = color_spaces.borrow();
                        borrowed
                            .as_any()
                            .downcast_ref::<CpdfDictionary>()?
                            .get_direct_object_for(&name)
                    })?;
                    return self.get_color_space_internal(&named, None, visited, visited_internal);
                }
                let stock = stock?;
                let Some(color_spaces) = named_resources else {
                    return Some(stock);
                };
                let default_name: &[u8] = match stock.family() {
                    ColorSpaceFamily::DeviceRgb => b"DefaultRGB",
                    ColorSpaceFamily::DeviceGray => b"DefaultGray",
                    ColorSpaceFamily::DeviceCmyk => b"DefaultCMYK",
                    _ => return Some(stock),
                };
                let default = {
                    let borrowed = color_spaces.borrow();
                    borrowed
                        .as_any()
                        .downcast_ref::<CpdfDictionary>()?
                        .get_direct_object_for(default_name)
                };
                return if let Some(default) = default {
                    self.get_color_space_internal(&default, None, visited, visited_internal)
                } else {
                    Some(stock)
                };
            }
            if kind != ObjectType::Array {
                return None;
            }
            let (size, first) = {
                let borrowed = object.borrow();
                let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
                (array.size(), array.get_direct_object_at(0))
            };
            if size == 0 {
                return None;
            }
            if size == 1 {
                return self.get_color_space_internal(
                    &first?,
                    resources,
                    visited,
                    visited_internal,
                );
            }
            if let Some(cached) = self
                .color_space_map
                .borrow()
                .get(&identity)
                .map(|(_, cached)| Rc::clone(cached))
            {
                return Some(cached);
            }
            let loaded = self.load_color_space_direct(&object, visited);
            if let Some(space) = &loaded {
                self.color_space_map
                    .borrow_mut()
                    .insert(identity, (object.clone(), Rc::clone(space)));
            }
            loaded
        })();
        visited_internal.remove(&identity);
        result
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn annotation_type3_font_uses_lazy_reverse_unicode_mapping() {
        use crate::fpdfapi::font::cpdf_font::CpdfFont;
        use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
        let objects = CpdfIndirectObjectHolder::new();
        let stream = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"1 beginbfchar <41> <20AC> endbfchar");
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"BaseFont", b"AnnotationFont");
        dict.set_for(b"ToUnicode", Some(make_reference(&stream, &objects)));
        let dict = retain_object(dict);
        let data = Rc::new(CpdfDocPageData::new());
        let font = CpdfFont::Type3(data.get_type3_font(&dict).unwrap());
        assert_eq!(font.base_font_name(), b"AnnotationFont");
        assert_eq!(font.char_code_from_unicode(0x20ac), 65);
        let mut bytes = Vec::new();
        font.append_char(&mut bytes, font.char_code_from_unicode(0x20ac));
        assert_eq!(bytes, b"A");
    }

    use super::*;

    #[test]
    fn icc_profiles_share_by_digest_and_components_and_nested_spaces_reuse_bases() {
        use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
        use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
        let mut bytes = vec![0; 3144];
        bytes[400..417].copy_from_slice(b"sRGB IEC61966-2.1");
        let make_stream = |components| {
            let mut dict = CpdfDictionary::new();
            dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(components));
            retain_object(CpdfStream::from_data_and_dict(
                bytes.clone(),
                retain_object(dict),
            ))
        };
        let stream = make_stream(3);
        let pointer = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_in_memory_raw_data()
            .as_ptr();
        let data = CpdfDocPageData::new();
        let profile = data.get_icc_profile(&stream).unwrap();
        assert!(profile.is_srgb());
        assert_eq!(profile.data().as_ptr(), pointer);
        let identical = make_stream(3);
        assert!(Rc::ptr_eq(
            &profile,
            &data.get_icc_profile(&identical).unwrap()
        ));
        assert_eq!(data.icc_profile_map.borrow().len(), 1);
        assert!(Rc::ptr_eq(profile.stream(), &stream));
        let different_components = data.get_icc_profile(&make_stream(4)).unwrap();
        assert!(!Rc::ptr_eq(&profile, &different_components));
        assert!(!different_components.is_valid());

        let objects = CpdfIndirectObjectHolder::new();
        let number = objects.borrow_mut().add_indirect_object(stream);
        let mut base = CpdfArray::new();
        base.append_new_name(b"ICCBased");
        base.append_new(CpdfReference::new(&objects, number));
        let base = retain_object(base);
        let loaded_base = data.get_color_space(&base, None).unwrap();
        let weak_base = Rc::downgrade(&loaded_base);
        let mut indexed = CpdfArray::new();
        indexed.append_new_name(b"Indexed");
        indexed.append(base);
        indexed.append_new(CpdfScalarObject::number_from_signed(0));
        indexed.append_new(CpdfString::from_bytes(None, &[0, 0, 0]));
        let indexed = retain_object(indexed);
        let loaded_indexed = data.get_color_space(&indexed, None).unwrap();
        drop(loaded_base);
        assert!(weak_base.upgrade().is_some());
        drop(loaded_indexed);
        assert!(weak_base.upgrade().is_some());
        drop(data);
        assert!(weak_base.upgrade().is_none());
    }

    #[test]
    fn shading_and_pattern_share_identity_and_native_failed_load_state() {
        use super::super::cpdf_pattern::PatternKind;
        use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
        use crate::fxcrt::fx_coordinates::CfxMatrix;

        let data = CpdfDocPageData::new();
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(2));
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        let object = retain_object(dict);
        let parent = CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 4.0, 5.0);
        let first = data.get_shading(&object, parent).unwrap();
        // Missing functions fail validation after setting the shading type.
        assert!(!first.load(&data));
        let second = data.get_shading(&object, CfxMatrix::default()).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert!(second.load(&data));
        assert_eq!(second.pattern().parent_matrix(), parent);
        let base = data.get_pattern(&object, CfxMatrix::default()).unwrap();
        assert_eq!(
            base.kind(),
            PatternKind::Shading {
                shading_object: true
            }
        );
        drop(first);
        drop(second);
        // A color's retained base keeps all loaded state without an Rc cycle.
        assert!(data.get_shading(&object, parent).unwrap().load(&data));
        drop(base);
        assert!(data.get_shading(&object, parent).unwrap().load(&data));

        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(1));
        let object = retain_object(dict);
        let _tiling = data.get_pattern(&object, parent).unwrap();
        assert!(data.get_shading(&object, parent).is_none());
    }
    use crate::fpdfapi::parser::cpdf_object::{make_reference, retain_object};
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    use crate::fxcodec::flate;
    use crate::fxge::cfx_face::CfxOwnedFace;
    use crate::fxge::fx_fontencoding::FontEncoding as FxFontEncoding;

    #[test]
    fn stock_font_aliases_share_a_face_and_clear_dictionary_at_teardown() {
        let page_data = CpdfDocPageData::new();
        let mut mapper = CfxFontMapper::new();
        let first = page_data.get_stock_font(b"Arial", &mut mapper).unwrap();
        let second = page_data.get_stock_font(b"Helvetica", &mut mapper).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(first.base_font_name(), b"Helvetica");
        assert!(first.font_dict().is_some());
        assert!(page_data
            .get_stock_font(b"NotAStandardFont", &mut mapper)
            .is_none());
        drop(page_data);
        assert!(first.font_dict().is_none());
        assert!(first.get_char_width(65) > 0);
    }

    #[test]
    fn type3_form_factory_handles_recursive_metrics_and_teardown() {
        let document = crate::fpdfapi::parser::cpdf_document::CpdfDocument::new();
        let holder = document.holder();
        let stream = holder.borrow_mut().new_indirect_stream_from_bytes(
            b"500 0 0 0 500 700 d1 0 0 500 700 re f BT /F1 1 Tf (A) Tj ET",
        );
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"Type3");
        dict.set_matrix_for(
            b"FontMatrix",
            crate::fxcrt::fx_coordinates::CfxMatrix::new(0.001, 0.0, 0.0, 0.001, 0.0, 0.0),
        );
        let encoding = dict.set_new_dictionary_for(b"Encoding");
        let differences = encoding
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_array_for(b"Differences");
        {
            let mut borrowed = differences.borrow_mut();
            let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
            array.append_new(CpdfScalarObject::number_from_signed(65));
            array.append_new_name(b"A");
        }
        let procs = dict.set_new_dictionary_for(b"CharProcs");
        procs
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"A", Some(make_reference(&stream, &holder)));
        let dict = retain_object(dict);
        let mut fonts = CpdfDictionary::new();
        fonts.set_for(b"F1", Some(Rc::clone(&dict)));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let page_data = Rc::new(CpdfDocPageData::new());
        let weak_page_data = Rc::downgrade(&page_data);
        let font = page_data.get_type3_font(&dict).unwrap();
        font.set_page_resources(Some(retain_object(resources)));
        // Parsing the character re-enters this font through Tf and metrics.
        font.check_type3_font_metrics();
        let glyph = font.load_char(65).unwrap();
        assert!(Rc::ptr_eq(&glyph, &font.load_char(65).unwrap()));
        assert_eq!(font.get_char_width(65), 500);
        assert_eq!(
            font.get_char_bbox(65),
            crate::fxcrt::fx_coordinates::FxRect::new(0, 700, 500, 0)
        );
        assert!(glyph.borrow().form().unwrap().has_page_objects());
        assert!(Rc::ptr_eq(&font, &page_data.get_type3_font(&dict).unwrap()));
        drop(page_data);
        assert!(weak_page_data.upgrade().is_none());
        assert!(glyph.borrow().form().is_none());
    }

    #[test]
    fn pdf_font_cache_retains_embedded_face_and_glyph_masks() {
        let page_data = CpdfDocPageData::new();
        let mut mapper = CfxFontMapper::new();
        let bytes =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let document = crate::fpdfapi::parser::cpdf_document::CpdfDocument::new();
        let holder = document.holder();
        let stream = holder.borrow_mut().new_indirect_stream_from_bytes(bytes);
        let mut descriptor = CpdfDictionary::new();
        descriptor.set_new_for(b"Flags", CpdfScalarObject::number_from_signed(32));
        descriptor.set_for(b"FontFile2", Some(make_reference(&stream, &holder)));
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"TrueType");
        dict.set_new_name_for(b"BaseFont", b"ABCDEF+Ahem");
        dict.set_for(b"FontDescriptor", Some(retain_object(descriptor)));
        let dict = retain_object(dict);
        let first = page_data.get_simple_font(&dict, &mut mapper).unwrap();
        assert!(first.is_embedded());
        assert_eq!(first.base_font_name(), b"Ahem");
        let glyph = first.base().glyph_from_char_code(65) as u32;
        let matrix = crate::fxcrt::fx_coordinates::CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0);
        let mask = first
            .font()
            .load_glyph_bitmap(
                glyph,
                false,
                matrix,
                0,
                crate::fxge::fx_font::FontAntiAliasingMode::Normal,
            )
            .unwrap();
        let weak = Rc::downgrade(&first);
        drop(first);
        let second = page_data.get_simple_font(&dict, &mut mapper).unwrap();
        assert!(Rc::ptr_eq(&weak.upgrade().unwrap(), &second));
        let reused_mask = second
            .font()
            .load_glyph_bitmap(
                glyph,
                false,
                matrix,
                0,
                crate::fxge::fx_font::FontAntiAliasingMode::Normal,
            )
            .unwrap();
        assert!(Rc::ptr_eq(&mask, &reused_mask));
        drop(page_data);
        assert_eq!(second.font().font_span(), bytes);
        drop(second);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn failed_embedded_font_is_purged_and_substituted_with_descriptor_style() {
        use crate::fxge::fx_font::{
            FONT_STYLE_ALL_CAP, FONT_STYLE_FIXED_PITCH, FONT_STYLE_NON_SYMBOLIC,
        };
        let page_data = CpdfDocPageData::new();
        let mut mapper = CfxFontMapper::new();
        let document = crate::fpdfapi::parser::cpdf_document::CpdfDocument::new();
        let holder = document.holder();
        let stream = holder
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"invalid font");
        let mut descriptor = CpdfDictionary::new();
        descriptor.set_for(b"FontFile2", Some(make_reference(&stream, &holder)));
        for (key, value) in [
            (
                b"Flags".as_slice(),
                (FONT_STYLE_ALL_CAP | FONT_STYLE_NON_SYMBOLIC) as i32,
            ),
            (b"FontWeight".as_slice(), 700),
            (b"ItalicAngle".as_slice(), -12),
            (b"Ascent".as_slice(), 800),
            (b"Descent".as_slice(), -200),
            (b"CapHeight".as_slice(), 700),
        ] {
            descriptor.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"TrueType");
        dict.set_new_name_for(b"BaseFont", b"MissingFont");
        dict.set_for(b"FontDescriptor", Some(retain_object(descriptor)));
        dict.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        let widths = dict.set_new_array_for(b"Widths");
        widths
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfScalarObject::number_from_signed(600));
        let font = page_data
            .get_simple_font(&retain_object(dict), &mut mapper)
            .unwrap();
        assert!(!font.is_embedded());
        assert!(page_data.font_file_map.borrow().is_empty());
        assert_ne!(font.font_base.flags & FONT_STYLE_FIXED_PITCH, 0);
        let subst = font.font().subst_font().unwrap();
        assert_eq!(subst.weight(), 700);
        assert_eq!(subst.italic_angle(), -12);
        assert_eq!(
            font.base().glyph_from_char_code(97),
            font.base().glyph_from_char_code(65)
        );
    }

    #[test]
    fn unusable_embedded_sfnt_is_purged_and_substituted_with_renderable_glyphs() {
        use crate::fxcrt::fx_coordinates::CfxMatrix;
        use crate::fxge::cfx_face::CfxFaceError;
        use crate::fxge::fx_font::FontAntiAliasingMode;
        use read_fonts::{types::Tag, FontRef};
        let bytes = include_bytes!("../../../tests/fixtures/hint-optional.ttf");
        let font = FontRef::new(bytes).unwrap();
        let record = font
            .table_directory()
            .table_records()
            .iter()
            .find(|record| record.tag.get() == Tag::new(b"glyf"))
            .unwrap();
        let truncated = &bytes[..record.offset() as usize + record.length() as usize / 2];
        let head = font
            .table_directory()
            .table_records()
            .iter()
            .find(|record| record.tag.get() == Tag::new(b"head"))
            .unwrap()
            .offset() as usize;
        let mut zero_em = bytes.to_vec();
        zero_em[head + 18..head + 20].copy_from_slice(&0u16.to_be_bytes());
        // Both an absent outline source and a present outline with no usable
        // em must reach the existing cache-purge/substitution/rendering path.
        for unusable in [truncated, zero_em.as_slice()] {
            let document = crate::fpdfapi::parser::cpdf_document::CpdfDocument::new();
            let holder = document.holder();
            let stream = holder.borrow_mut().new_indirect_stream_from_bytes(unusable);
            let page_data = CpdfDocPageData::new();
            assert_eq!(
                page_data.get_font_file_face(&stream).err(),
                Some(CfxFaceError::InvalidFont)
            );
            assert!(page_data.font_file_map.borrow().is_empty());
            assert!(page_data.font_face_map.borrow().is_empty());
            let mut descriptor = CpdfDictionary::new();
            descriptor.set_new_for(b"Flags", CpdfScalarObject::number_from_signed(32));
            descriptor.set_for(b"FontFile2", Some(make_reference(&stream, &holder)));
            let mut dict = CpdfDictionary::new();
            dict.set_new_name_for(b"Subtype", b"TrueType");
            dict.set_new_name_for(b"BaseFont", b"ABCDEF+Calibri");
            dict.set_for(b"FontDescriptor", Some(retain_object(descriptor)));
            dict.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
            dict.set_new_array_for(b"Widths")
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfArray>()
                .unwrap()
                .append_new(CpdfScalarObject::number_from_signed(600));
            let dict = retain_object(dict);
            let mut mapper = CfxFontMapper::new();
            let loaded = page_data.get_simple_font(&dict, &mut mapper).unwrap();
            assert!(!loaded.is_embedded());
            assert!(loaded
                .font()
                .subst_font()
                .unwrap()
                .is_builtin_generic_font());
            assert!(page_data.font_file_map.borrow().is_empty());
            assert!(page_data.font_face_map.borrow().is_empty());
            let glyph = loaded.base().glyph_from_char_code(65) as u32;
            assert_ne!(glyph, 0);
            let bitmap = loaded
                .font()
                .load_glyph_bitmap(
                    glyph,
                    false,
                    CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0),
                    600,
                    FontAntiAliasingMode::Normal,
                )
                .unwrap();
            assert!(bitmap.bitmap().buffer().iter().any(|&v| v != 0));
            assert!(Rc::ptr_eq(
                &loaded,
                &page_data.get_simple_font(&dict, &mut mapper).unwrap()
            ));
        }
    }

    #[test]
    fn font_file_cache_shares_raw_bytes_and_purges_after_last_caller() {
        let stream = retain_object(CpdfStream::from_bytes(b"font bytes"));
        let source_ptr = {
            let borrowed = stream.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfStream>()
                .unwrap()
                .get_in_memory_raw_data()
                .as_ptr()
        };
        let page_data = CpdfDocPageData::new();
        let first = page_data.get_font_file_data(&stream).unwrap();
        assert_eq!(first.data(), b"font bytes");
        assert_eq!(first.data().as_ptr(), source_ptr);
        let second = page_data.get_font_file_data(&stream).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        let weak = Rc::downgrade(&first);
        page_data.maybe_purge_font_file_data(first);
        assert!(weak.upgrade().is_some());
        page_data.maybe_purge_font_file_data(second);
        assert!(weak.upgrade().is_none());

        let compressed = flate::encode(b"decoded font bytes");
        let filtered = retain_object(CpdfStream::from_bytes(&compressed));
        {
            let borrowed = filtered.borrow();
            let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
            stream
                .get_dict()
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_name_for(b"Filter", b"FlateDecode");
        }
        let decoded = page_data.get_font_file_data(&filtered).unwrap();
        assert_eq!(decoded.data(), b"decoded font bytes");

        let fallback = retain_object(CpdfStream::from_bytes(b"raw font fallback"));
        let pointer = {
            let borrowed = fallback.borrow();
            let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
            stream
                .get_dict()
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_name_for(b"Filter", b"UnknownFilter");
            stream.get_in_memory_raw_data().as_ptr()
        };
        let cached = page_data.get_font_file_data(&fallback).unwrap();
        assert_eq!(cached.data(), b"raw font fallback");
        assert_eq!(cached.data().as_ptr(), pointer);
    }

    #[test]
    fn owned_face_retains_cached_font_bytes_and_selected_charmap() {
        let bytes =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let stream = retain_object(CpdfStream::from_bytes(bytes));
        let page_data = CpdfDocPageData::new();
        let cached = page_data.get_font_file_data(&stream).unwrap();
        let bytes_ptr = cached.data().as_ptr();
        let weak = Rc::downgrade(&cached);
        let mut face = CfxOwnedFace::new(Rc::clone(&cached), 0).unwrap();
        page_data.maybe_purge_font_file_data(cached);
        assert!(weak.upgrade().is_some());
        assert_eq!(face.face().data().as_ptr(), bytes_ptr);
        assert!(face.with_face_mut(|face| face.select_charmap(FxFontEncoding::Unicode)));
        assert!(face.face().get_char_index(b'A' as u32) > 0);
        drop(face);
        page_data.maybe_purge_font_file_data(weak.upgrade().unwrap());
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn embedded_face_cache_reuses_parsed_face_and_retains_its_bytes() {
        let bytes = include_bytes!("../../../tests/fixtures/triangle.pfa");
        let stream = retain_object(CpdfStream::from_bytes(bytes));
        let page_data = CpdfDocPageData::new();
        let first = page_data.get_font_file_face(&stream).unwrap().unwrap();
        let second = page_data.get_font_file_face(&stream).unwrap().unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        let file = page_data.get_font_file_data(&stream).unwrap();
        assert_eq!(first.face().data().as_ptr(), file.data().as_ptr());
        assert!(first.face().select_charmap(FxFontEncoding::AdobeCustom));
        assert_eq!(
            second.face().current_charmap_encoding(),
            FxFontEncoding::AdobeCustom
        );
        drop(page_data);
        drop(file);
        assert!(first.face().get_char_index(u32::from(b'A')) > 0);
    }

    #[test]
    fn resource_names_defaults_and_retained_array_cache() {
        let mut spaces = CpdfDictionary::new();
        spaces.set_new_name_for(b"Alias", b"DeviceGray");
        spaces.set_new_name_for(b"DefaultRGB", b"DeviceGray");
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"ColorSpace", spaces);
        let resources = retain_object(resources);
        let data = CpdfDocPageData::new();
        let alias = retain_object(crate::fpdfapi::parser::cpdf_name::CpdfName::new(
            None, b"Alias",
        ));
        let rgb = retain_object(crate::fpdfapi::parser::cpdf_name::CpdfName::new(
            None,
            b"DeviceRGB",
        ));
        assert_eq!(
            data.get_color_space(&alias, Some(&resources))
                .unwrap()
                .family(),
            ColorSpaceFamily::DeviceGray
        );
        assert_eq!(
            data.get_color_space(&rgb, Some(&resources))
                .unwrap()
                .family(),
            ColorSpaceFamily::DeviceGray
        );
        assert_eq!(
            data.get_color_space(&rgb, None).unwrap().family(),
            ColorSpaceFamily::DeviceRgb
        );

        let mut array = CpdfArray::new();
        array.append_new_name(b"Indexed");
        array.append_new_name(b"DeviceRGB");
        array.append_new(CpdfScalarObject::number_from_signed(0));
        array.append_new(CpdfString::from_bytes(None, &[255, 0, 0]));
        let array = retain_object(array);
        let first = data.get_color_space(&array, None).unwrap();
        let second = data.get_color_space(&array, None).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(second.get_rgb(&[0.0]).unwrap().red, 1.0);
        let weak = Rc::downgrade(&first);
        drop(first);
        drop(second);
        assert!(weak.upgrade().is_some());
        drop(data);
        assert!(weak.upgrade().is_none());
    }
}

fn native_font_dict_mut<R>(dict: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut borrowed = dict.borrow_mut();
    f(borrowed
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("font dictionary"))
}

fn set_number(dict: &mut CpdfDictionary, key: &[u8], number: i32) {
    dict.set_new_for(key, CpdfScalarObject::number_from_signed(number));
}
fn push_number(array: &mut CpdfArray, number: i32) {
    array.append_new(CpdfScalarObject::number_from_signed(number));
}
fn set_reference(dict: &ObjectRef, key: &[u8], objects: &HolderRef, target: &ObjectRef) {
    let number = target.borrow().obj_num();
    native_font_dict_mut(dict, |d| {
        d.set_new_for(key, CpdfReference::new(objects, number));
    });
}
/// `CPDF_DocPageData::AddFont`, ProcessNonbCJK, CalculateEncodingDict,
/// ProcessbCJK, InsertWidthArray1 and CalculateFontDesc, portable branch.
fn add_native_font_dictionary(
    objects: &HolderRef,
    font: &CfxFont<'_>,
    charset: FxCharset,
) -> ObjectRef {
    let mut basefont = font.family_name().into_bytes();
    basefont.retain(|&c| c != b' ');
    let mut flags = if charset == FxCharset::Symbol {
        FONT_STYLE_SYMBOLIC
    } else {
        FONT_STYLE_NON_SYMBOLIC
    };
    if font.is_bold() {
        flags |= FONT_STYLE_FORCE_BOLD;
    }
    if font.is_italic() {
        flags |= FONT_STYLE_ITALIC;
    }
    if font.is_fixed_width() {
        flags |= FONT_STYLE_FIXED_PITCH;
    }
    let base = objects.borrow_mut().new_indirect_dictionary();
    native_font_dict_mut(&base, |d| {
        d.set_new_name_for(b"Type", b"Font");
    });
    // CFX_CharmapResolver::CreateUnicode/GlyphFromCharCode. Face charmap
    // selection uses interior state, so no cloned face or font bytes are needed.
    let width = |word: u32| {
        let glyph = font.face().map_or(word, |face| {
            use crate::fxge::fx_fontencoding::FontEncoding as FaceEncoding;
            if face.select_charmap(FaceEncoding::Unicode) {
                return face.get_char_index(word);
            }
            if font.subst_font().is_some_and(|font| font.is_symbolic()) {
                let index = if face.select_charmap(FaceEncoding::Symbol) {
                    face.get_char_index(word)
                } else {
                    0
                };
                if index == 0 && face.select_charmap(FaceEncoding::AppleRoman) {
                    return face.get_char_index(word);
                }
            }
            word
        });
        font.get_glyph_width(glyph, 0)
    };
    let font_dict = if !charset_is_cjk(charset) {
        let mut widths = CpdfArray::new();
        for word in 32..128 {
            push_number(&mut widths, width(word));
        }
        if matches!(
            charset,
            FxCharset::Ansi | FxCharset::Default | FxCharset::Symbol
        ) {
            native_font_dict_mut(&base, |d| {
                d.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
            });
            for word in 128..=255 {
                push_number(&mut widths, width(word));
            }
        } else if let Some((_, unicodes)) = CHARSET_UNICODES.iter().find(|(c, _)| *c == charset) {
            let encoding = objects.borrow_mut().new_indirect_dictionary();
            native_font_dict_mut(&encoding, |d| {
                d.set_new_name_for(b"BaseEncoding", b"WinAnsiEncoding");
                let mut differences = CpdfArray::new();
                push_number(&mut differences, 128);
                for &unicode in unicodes.iter() {
                    let name = adobe_name_from_unicode(u32::from(unicode));
                    differences.append_new_name(if name.is_empty() { b".notdef" } else { &name });
                    push_number(&mut widths, width(u32::from(unicode)));
                }
                d.set_new_for(b"Differences", differences);
            });
            set_reference(&base, b"Encoding", objects, &encoding);
        }
        let mut styled = basefont.clone();
        if font.is_bold() && font.is_italic() {
            styled.extend_from_slice(b",BoldItalic");
        } else if font.is_bold() {
            styled.extend_from_slice(b",Bold");
        } else if font.is_italic() {
            styled.extend_from_slice(b",Italic");
        }
        native_font_dict_mut(&base, |d| {
            d.set_new_name_for(b"Subtype", b"TrueType");
            d.set_new_name_for(b"BaseFont", &styled);
            set_number(d, b"FirstChar", 32);
            set_number(d, b"LastChar", 255);
            d.set_new_for(b"Widths", widths);
        });
        Rc::clone(&base)
    } else {
        let cid = objects.borrow_mut().new_indirect_dictionary();
        let mut widths = CpdfArray::new();
        let mut insert = |first: i32, start: u32, end: u32| {
            push_number(&mut widths, first);
            let values: Vec<i32> = (start..=end).map(width).collect();
            if values.iter().all(|&v| v == values[0]) {
                push_number(&mut widths, first + values.len() as i32 - 1);
                push_number(&mut widths, values[0]);
            } else {
                let mut array = CpdfArray::new();
                for value in values {
                    push_number(&mut array, value);
                }
                widths.append_new(array);
            }
        };
        let (cmap, ordering, supplement): (&[u8], &[u8], i32) = match charset {
            FxCharset::ChineseTraditional => {
                insert(1, 0x20, 0x7e);
                (b"ETenms-B5-H", b"CNS1", 4)
            }
            FxCharset::ChineseSimplified => {
                insert(7716, 0x20, 0x20);
                insert(814, 0x21, 0x7e);
                (b"GBK-EUC-H", b"GB1", 2)
            }
            FxCharset::Hangul => {
                insert(1, 0x20, 0x7e);
                (b"KSCms-UHC-H", b"Korea1", 2)
            }
            FxCharset::ShiftJis => {
                insert(231, 0x20, 0x7d);
                insert(326, 0xa0, 0xa0);
                insert(327, 0xa1, 0xdf);
                insert(631, 0x7e, 0x7e);
                (b"90ms-RKSJ-H", b"Japan1", 5)
            }
            _ => (b"", b"", 0),
        };
        native_font_dict_mut(&base, |d| {
            d.set_new_name_for(b"Subtype", b"Type0");
            d.set_new_name_for(b"BaseFont", &basefont);
            d.set_new_name_for(b"Encoding", cmap);
            let mut descendants = CpdfArray::new();
            descendants.append_new(CpdfReference::new(objects, cid.borrow().obj_num()));
            d.set_new_for(b"DescendantFonts", descendants);
        });
        native_font_dict_mut(&cid, |d| {
            d.set_new_name_for(b"Type", b"Font");
            d.set_new_name_for(b"Subtype", b"CIDFontType2");
            d.set_new_name_for(b"BaseFont", &basefont);
            d.set_new_for(b"W", widths);
            let mut info = CpdfDictionary::new();
            info.set_new_string_for(b"Registry", b"Adobe");
            info.set_new_string_for(b"Ordering", ordering);
            set_number(&mut info, b"Supplement", supplement);
            d.set_new_for(b"CIDSystemInfo", info);
        });
        cid
    };
    let stem = font.subst_font().map_or_else(
        || {
            let mut stem = width(u32::from(b'i'));
            for word in [b'I', b'!', b'1'] {
                let value = width(u32::from(word));
                if value > 0 && value < stem {
                    stem = value;
                }
            }
            stem
        },
        |s| s.estimated_stem_v(),
    );
    let descriptor = objects.borrow_mut().new_indirect_dictionary();
    native_font_dict_mut(&descriptor, |d| {
        d.set_new_name_for(b"Type", b"FontDescriptor");
        d.set_new_name_for(b"FontName", &basefont);
        set_number(d, b"Flags", flags as i32);
        let bbox = font.bbox().unwrap_or_default();
        d.set_rect_for(
            b"FontBBox",
            CfxFloatRect::new(
                bbox.left as f32,
                bbox.bottom as f32,
                bbox.right as f32,
                bbox.top as f32,
            ),
        );
        set_number(d, b"ItalicAngle", font.subst_font_italic_angle());
        set_number(d, b"Ascent", font.get_ascent());
        set_number(d, b"Descent", font.get_descent());
        set_number(d, b"StemV", stem);
    });
    set_reference(&font_dict, b"FontDescriptor", objects, &descriptor);
    base
}
