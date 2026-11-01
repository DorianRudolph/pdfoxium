// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Loading and page-tree access from `core/fpdfapi/parser/cpdf_document.{h,cpp}`.
//! Includes new-document/page insertion, deletion and moves; XFA hooks remain open.
//! Static resource caches and appearance-stream bookkeeping are owned by
//! document page/render data.

use std::collections::HashSet;
use std::rc::Rc;

use super::cpdf_array::CpdfArray;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use super::cpdf_object::ObjectRef;
use super::cpdf_parser::{CpdfParser, ParserError};
use super::cpdf_read_validator::CpdfReadValidator;
use super::cpdf_reference::CpdfReference;
use super::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::page::cpdf_docpagedata::{CachedFontFile, CpdfDocPageData};
use crate::fpdfapi::render::cpdf_docrenderdata::CpdfDocRenderData;
use crate::fxcrt::fx_stream::SeekableReadStream;

const MAX_PAGE_LEVEL: usize = 1024;
pub const PAGE_MAX_NUM: usize = 0xFFFFF;

#[derive(Clone, Copy, Eq, PartialEq)]
enum NodeType {
    Branch,
    Leaf,
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{with_dict, CpdfDocument};
    use crate::fpdfapi::parser::cpdf_parser::ParserError;
    use crate::fpdfapi::parser::cpdf_read_validator::CpdfReadValidator;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    #[test]
    fn loads_two_page_pdf_and_resolves_pages_out_of_order() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
        ));
        let mut document = CpdfDocument::new();
        assert_eq!(
            document.load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
                b""
            ),
            ParserError::Success
        );
        assert_eq!(document.page_count(), 2);
        assert!(Rc::ptr_eq(
            &document.holder(),
            &document.page_data().document_objects().unwrap()
        ));
        assert!(Rc::ptr_eq(
            &document.root().unwrap(),
            &document.page_data().document_root().unwrap()
        ));
        assert!(document.has_valid_cross_reference_table());
        assert!(!document.is_page_loaded(0));
        assert_eq!(document.page_index(4), Some(1));
        assert_eq!(document.page_dictionary(1).unwrap().borrow().obj_num(), 4);
        assert_eq!(document.page_dictionary(0).unwrap().borrow().obj_num(), 3);
        assert_eq!(document.page_index(3), Some(0));
        assert_eq!(document.page_index(100), None);
        assert!(document.page_dictionary(2).is_none());
        assert!(document.is_page_loaded(0));
    }

    #[test]
    fn document_init_reserves_unparsed_xref_numbers_for_appearance_objects() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
        ));
        let mut document = CpdfDocument::new();
        assert_eq!(
            document.load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
                b""
            ),
            ParserError::Success
        );
        let last = document.parser().last_obj_num();
        let holder = document.holder();
        let new = holder.borrow_mut().new_indirect_dictionary();
        assert_eq!(new.borrow().obj_num(), last + 1);
    }

    #[test]
    fn new_document_keeps_info_separate_and_binds_resource_ownership() {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let root = document.root().unwrap();
        assert_eq!(root.borrow().obj_num(), 1);
        assert_eq!(document.page_count(), 0);
        let info = document.info().unwrap();
        assert_eq!(info.borrow().obj_num(), 3);
        with_dict(&info, |dict| {
            dict.with_locked_entries(|entries| assert!(entries.is_empty()))
        });
        assert!(Rc::ptr_eq(
            &root,
            &document.page_data().document_root().unwrap(),
        ));
        assert!(Rc::ptr_eq(
            &document.holder(),
            &document.page_data().document_objects().unwrap(),
        ));
    }

    #[test]
    fn linearized_first_page_init_does_not_require_main_page_tree_bytes() {
        use crate::fpdfapi::parser::cpdf_read_validator::FileAvail;
        struct FirstSection;
        impl FileAvail for FirstSection {
            fn is_data_avail(&self, offset: i64, size: usize) -> bool {
                offset >= 0 && offset + size as i64 <= 4096
            }
        }
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/linearized.pdf"
        ));
        let available: Rc<dyn FileAvail> = Rc::new(FirstSection);
        let validator = Rc::new(CpdfReadValidator::new(
            Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
            Some(&available),
        ));
        let mut document = CpdfDocument::new();
        assert_eq!(
            document.load_linearized_doc(Rc::clone(&validator), b""),
            ParserError::Success
        );
        assert_eq!(document.page_count(), 3);
        assert!(document.page_dictionary(0).is_some());
        assert!(!validator.has_unavailable_data());
    }

    #[test]
    fn cached_page_and_info_entries_require_dictionaries_rather_than_streams() {
        use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
        use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
        ));
        let mut document = CpdfDocument::new();
        assert_eq!(
            document.load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
                b""
            ),
            ParserError::Success
        );
        let holder = document.holder();
        // Object 7 is a content stream. ToDictionary must not return its /Dict.
        document.page_list[0] = 7;
        assert_eq!(document.page_dictionary(0).unwrap().borrow().obj_num(), 3);
        let trailer = document.parser().trailer().unwrap();
        trailer
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Info", CpdfReference::new(&holder, 7));
        assert!(document.info().is_none());
    }

    #[test]
    fn repairs_invalid_page_count_and_missing_page_type() {
        let original = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
        ));
        let mut bytes = original.to_vec();
        let count = bytes
            .windows(8)
            .position(|part| part == b"/Count 2")
            .unwrap();
        bytes[count + 7] = b'0';
        let page = bytes
            .windows(11)
            .position(|part| part == b"/Type /Page")
            .unwrap();
        // The first occurrence is /Pages. Locate an exact leaf token.
        let page = (page..bytes.len() - 12)
            .find(|&i| &bytes[i..i + 12] == b"/Type /Page\n")
            .unwrap();
        bytes[page + 7..page + 11].copy_from_slice(b"None");
        let mut document = CpdfDocument::new();
        assert_eq!(
            document.load_doc(Rc::new(CfxReadOnlyContainerStream::new(bytes)), b""),
            ParserError::Success
        );
        assert_eq!(document.page_count(), 2);
        assert_eq!(document.page_dictionary(0).unwrap().borrow().obj_num(), 3);
        let pages = document.pages_dict().unwrap();
        assert_eq!(
            with_dict(&pages, |dict| dict.get_integer_for(b"Count", -1)),
            Some(2)
        );
    }

    #[test]
    fn preserves_linearized_first_page_lookup() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/linearized.pdf"
        ));
        let file = Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec()));
        let validator = Rc::new(CpdfReadValidator::new(file, None));
        let mut document = CpdfDocument::new();
        assert_eq!(
            document.load_linearized_doc(validator, b""),
            ParserError::Success
        );
        let header = document.parser().linearized_header().unwrap();
        assert_eq!(document.page_count(), header.page_count() as usize);
        assert!(document.is_page_loaded(header.first_page_no() as usize));
        assert!(document
            .page_dictionary(header.first_page_no() as usize)
            .is_some());
    }
}

fn object_identity(object: &ObjectRef) -> usize {
    Rc::as_ptr(object) as *const () as usize
}

fn with_dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> Option<R> {
    let borrowed = object.borrow();
    Some(f(borrowed.as_any().downcast_ref::<CpdfDictionary>()?))
}

fn with_dict_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> Option<R> {
    let mut borrowed = object.borrow_mut();
    Some(f(borrowed.as_any_mut().downcast_mut::<CpdfDictionary>()?))
}

fn with_array<R>(object: &ObjectRef, f: impl FnOnce(&CpdfArray) -> R) -> Option<R> {
    let borrowed = object.borrow();
    Some(f(borrowed.as_any().downcast_ref::<CpdfArray>()?))
}

fn node_type(object: &ObjectRef) -> Option<NodeType> {
    with_dict_mut(object, |dict| {
        match crate::fxcrt::bytestring::c_str(&dict.get_name_for(b"Type")) {
            b"Pages" => NodeType::Branch,
            b"Page" => NodeType::Leaf,
            _ => {
                let branch = dict.key_exist(b"Kids");
                dict.set_new_name_for(b"Type", if branch { b"Pages" } else { b"Page" });
                if branch {
                    NodeType::Branch
                } else {
                    NodeType::Leaf
                }
            }
        }
    })
}

fn count_pages(pages: &ObjectRef, visited: &mut HashSet<usize>) -> Option<usize> {
    let count = with_dict(pages, |dict| dict.get_integer_for(b"Count", 0))?;
    if count > 0 && (count as usize) < PAGE_MAX_NUM {
        return Some(count as usize);
    }
    let Some(kids) = with_dict(pages, |dict| dict.get_array_for(b"Kids"))? else {
        return Some(0);
    };
    let len = with_array(&kids, CpdfArray::size)?;
    let mut count = 0usize;
    for index in 0..len {
        let Some(kid) = with_array(&kids, |array| array.get_dict_at(index))? else {
            continue;
        };
        let identity = object_identity(&kid);
        if visited.contains(&identity) {
            continue;
        }
        match node_type(&kid)? {
            NodeType::Branch => {
                visited.insert(identity);
                let child_count = count_pages(&kid, visited);
                visited.remove(&identity);
                count = count.checked_add(child_count?)?;
            }
            NodeType::Leaf => count += 1,
        }
        if count >= PAGE_MAX_NUM {
            return None;
        }
    }
    with_dict_mut(pages, |dict| {
        dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(count as i32));
    })?;
    Some(count)
}

fn find_page_index(
    node: &ObjectRef,
    skip_count: &mut usize,
    obj_num: u32,
    index: &mut usize,
    level: usize,
) -> Option<usize> {
    let has_kids = with_dict(node, |dict| dict.key_exist(b"Kids"))?;
    if !has_kids {
        if node.borrow().obj_num() == obj_num {
            return Some(*index);
        }
        *skip_count = skip_count.saturating_sub(1);
        *index += 1;
        return None;
    }
    let kids = with_dict(node, |dict| dict.get_array_for(b"Kids"))??;
    if level >= MAX_PAGE_LEVEL {
        return None;
    }
    let count = with_dict(node, |dict| dict.get_integer_for(b"Count", 0))? as usize;
    if count <= *skip_count {
        *skip_count -= count;
        *index = index.saturating_add(count);
        return None;
    }
    let len = with_array(&kids, CpdfArray::size)?;
    if count != 0 && count == len {
        for i in 0..count {
            let Some(child) = with_array(&kids, |array| array.get_object_at(i))? else {
                continue;
            };
            let borrowed = child.borrow();
            if borrowed
                .as_any()
                .downcast_ref::<CpdfReference>()
                .is_some_and(|reference| reference.ref_obj_num() == obj_num)
            {
                return Some(*index + i);
            }
        }
    }
    for i in 0..len {
        let Some(child) = with_array(&kids, |array| array.get_dict_at(i))? else {
            continue;
        };
        if Rc::ptr_eq(&child, node) {
            continue;
        }
        if let Some(found) = find_page_index(&child, skip_count, obj_num, index, level + 1) {
            return Some(found);
        }
    }
    None
}

struct PageTreeData {
    pages_dict: ObjectRef,
    child_index: usize,
}

pub struct CpdfDocument {
    page_data: Rc<CpdfDocPageData>,
    render_data: Rc<CpdfDocRenderData>,
    page_list: Vec<u32>,
    tree_traversal: Vec<PageTreeData>,
    info_dict: Option<ObjectRef>,
    root_dict: Option<ObjectRef>,
    optional_content_generation: u64,
    optional_content: std::cell::OnceCell<
        Option<Rc<std::cell::RefCell<crate::fpdfapi::page::optional_content::OcState>>>,
    >,
    // Keep parser/indirect-object storage alive through resource/tree teardown.
    parser: CpdfParser,
    reached_max_page_level: bool,
    next_page_to_traverse: i32,
    has_valid_cross_reference_table: bool,
}

impl Default for CpdfDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl CpdfDocument {
    pub fn new() -> Self {
        let mut parser = CpdfParser::new();
        parser.set_try_init(Some(Rc::new(|parser| {
            // CPDF_Document::TryInit reserves every existing xref number before
            // page repair or generated annotation appearances add objects.
            let holder = parser.holder();
            holder.borrow_mut().set_last_obj_num(parser.last_obj_num());
            let Some(root) = parser.root() else {
                return false;
            };
            if let Some(header) = parser.linearized_header() {
                let first = CpdfIndirectObjectHolder::get_or_parse_indirect_object(
                    &holder,
                    header.first_page_obj_num(),
                );
                if first.as_ref().is_some_and(Self::is_valid_page_object) {
                    // LoadPages uses the linearized first page without reading
                    // the main page tree, which may not yet be available.
                    return header.page_count() > 0;
                }
            }
            Self::retrieve_page_count_from_root(&root) > 0
        })));
        let page_data = Rc::new(CpdfDocPageData::new());
        page_data.bind_document_resources(&parser.holder(), None);
        let render_data = page_data.render_data_ref();
        Self {
            page_data,
            render_data,
            parser,
            root_dict: None,
            optional_content_generation: 0,
            optional_content: Default::default(),
            info_dict: None,
            page_list: Vec::new(),
            tree_traversal: Vec::new(),
            reached_max_page_level: false,
            next_page_to_traverse: 0,
            has_valid_cross_reference_table: false,
        }
    }

    /// Transient optional-content state, created only for documents with OCGs.
    pub fn optional_content(
        &self,
    ) -> Option<Rc<std::cell::RefCell<crate::fpdfapi::page::optional_content::OcState>>> {
        self.optional_content
            .get_or_init(|| {
                crate::fpdfapi::page::optional_content::OcModel::load(self.root()).map(|model| {
                    Rc::new(std::cell::RefCell::new(
                        crate::fpdfapi::page::optional_content::OcState::new(model),
                    ))
                })
            })
            .clone()
    }
    pub fn optional_content_generation(&self) -> u64 {
        self.optional_content_generation
    }
    pub fn reset_optional_content(&mut self) {
        self.optional_content.take();
        self.optional_content_generation = self.optional_content_generation.saturating_add(1);
    }
    pub fn parser(&self) -> &CpdfParser {
        &self.parser
    }
    pub fn page_data(&self) -> &CpdfDocPageData {
        &self.page_data
    }
    pub fn page_data_ref(&self) -> Rc<CpdfDocPageData> {
        Rc::clone(&self.page_data)
    }

    pub fn get_font_file_data(&self, stream: &ObjectRef) -> Option<Rc<CachedFontFile>> {
        self.page_data.get_font_file_data(stream)
    }

    pub fn maybe_purge_font_file_data(&self, cached: Rc<CachedFontFile>) {
        self.page_data.maybe_purge_font_file_data(cached);
    }
    pub fn render_data(&self) -> &CpdfDocRenderData {
        &self.render_data
    }
    pub fn render_data_ref(&self) -> Rc<CpdfDocRenderData> {
        Rc::clone(&self.render_data)
    }
    pub fn parser_mut(&mut self) -> &mut CpdfParser {
        &mut self.parser
    }
    pub fn holder(&self) -> HolderRef {
        self.parser.holder()
    }
    pub fn root(&self) -> Option<ObjectRef> {
        self.root_dict.clone()
    }

    /// `CPDF_Document::CreateNewDoc`: create the catalog, empty page tree and
    /// info dictionary. SDK metadata and timestamps are supplied by its caller.
    pub fn create_new_doc(&mut self) {
        self.reset_optional_content();
        debug_assert!(self.root_dict.is_none());
        debug_assert!(self.info_dict.is_none());
        let holder = self.holder();
        let root = holder.borrow_mut().new_indirect_dictionary();
        with_dict_mut(&root, |dict| {
            dict.set_new_for(b"Type", super::cpdf_name::CpdfName::new(None, b"Catalog"));
        });
        let pages = holder.borrow_mut().new_indirect_dictionary();
        with_dict_mut(&pages, |dict| {
            dict.set_new_for(b"Type", super::cpdf_name::CpdfName::new(None, b"Pages"));
            dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(0));
            dict.set_new_for(b"Kids", CpdfArray::new());
        });
        with_dict_mut(&root, |dict| {
            dict.set_new_for(
                b"Pages",
                CpdfReference::new(&holder, pages.borrow().obj_num()),
            );
        });
        self.root_dict = Some(root);
        self.info_dict = Some(holder.borrow_mut().new_indirect_dictionary());
        self.page_data
            .bind_document_resources(&holder, self.root_dict.as_ref());
    }

    /// `CreateNewPage`: allocate first, discarding the new indirect object if
    /// the requested insertion cannot be made. Object numbers are not reused.
    pub fn create_new_page(&mut self, index: i32) -> Option<ObjectRef> {
        let holder = self.holder();
        let page = holder.borrow_mut().new_indirect_dictionary();
        with_dict_mut(&page, |dict| dict.set_new_name_for(b"Type", b"Page"));
        let number = page.borrow().obj_num();
        if !self.insert_new_page(index, &page) {
            holder.borrow_mut().delete_indirect_object(number);
            return None;
        }
        Some(page)
    }

    fn insert_delete_pdf_page(
        &mut self,
        pages: &ObjectRef,
        mut pages_to_go: i32,
        page: Option<&ObjectRef>,
        is_insert: bool,
        visited: &mut HashSet<usize>,
    ) -> bool {
        let Some(kids) = with_dict(pages, |dict| dict.get_array_for(b"Kids")).flatten() else {
            return false;
        };
        let mut index = 0;
        while index < with_array(&kids, CpdfArray::size).unwrap() {
            let kid = with_array(&kids, |array| array.get_dict_at(index))
                .flatten()
                .expect("page-tree child must be a dictionary");
            let kind = node_type(&kid).expect("page-tree dictionary");
            if kind == NodeType::Leaf {
                if pages_to_go != 0 {
                    pages_to_go = pages_to_go.checked_sub(1).expect("page index overflow");
                    index += 1;
                    continue;
                }
                if is_insert {
                    let page = page.expect("insert requires a page");
                    kids.borrow_mut()
                        .as_any_mut()
                        .downcast_mut::<CpdfArray>()
                        .unwrap()
                        .insert_new_at(
                            index,
                            CpdfReference::new(&self.holder(), page.borrow().obj_num()),
                        );
                    with_dict_mut(page, |dict| {
                        dict.set_new_for(
                            b"Parent",
                            CpdfReference::new(&self.holder(), pages.borrow().obj_num()),
                        );
                    });
                } else {
                    kids.borrow_mut()
                        .as_any_mut()
                        .downcast_mut::<CpdfArray>()
                        .unwrap()
                        .remove_at(index);
                }
                Self::adjust_page_count(pages, is_insert);
                self.reset_traversal();
                break;
            }

            let count = with_dict(&kid, |dict| dict.get_integer_for(b"Count", 0)).unwrap();
            if pages_to_go >= count {
                pages_to_go = pages_to_go.checked_sub(count).expect("page count overflow");
                index += 1;
                continue;
            }
            let identity = object_identity(&kid);
            if !visited.insert(identity) {
                return false;
            }
            let success = self.insert_delete_pdf_page(&kid, pages_to_go, page, is_insert, visited);
            visited.remove(&identity);
            if !success {
                return false;
            }
            Self::adjust_page_count(pages, is_insert);
            break;
        }
        // Preserve the source's success return even when no child was selected.
        true
    }

    fn adjust_page_count(pages: &ObjectRef, is_insert: bool) {
        with_dict_mut(pages, |dict| {
            let count = dict.get_integer_for(b"Count", 0);
            let count = count
                .checked_add(if is_insert { 1 } else { -1 })
                .expect("page count overflow");
            dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(count));
        });
    }

    fn insert_new_page(&mut self, index: i32, page: &ObjectRef) -> bool {
        let Some(pages) = self.pages_dict() else {
            return false;
        };
        let count = self.page_count();
        if index < 0 || index as usize > count {
            return false;
        }
        if index as usize == count {
            // Resolve the existing value before borrowing the dictionary
            // mutably: /Kids may refer back to this same dictionary. Native
            // GetOrCreateArrayFor replaces that wrong-typed value with an array.
            let kids = with_dict(&pages, |dict| dict.get_array_for(b"Kids"))
                .flatten()
                .unwrap_or_else(|| {
                    with_dict_mut(&pages, |dict| {
                        dict.set_new_for(b"Kids", CpdfArray::with_pool(dict.byte_string_pool()))
                    })
                    .unwrap()
                });
            kids.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfArray>()
                .unwrap()
                .append_new(CpdfReference::new(&self.holder(), page.borrow().obj_num()));
            with_dict_mut(&pages, |dict| {
                let count = i32::try_from(count)
                    .expect("page count width")
                    .checked_add(1)
                    .expect("page count overflow");
                dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(count));
            });
            with_dict_mut(page, |dict| {
                dict.set_new_for(
                    b"Parent",
                    CpdfReference::new(&self.holder(), pages.borrow().obj_num()),
                );
            });
            self.reset_traversal();
        } else {
            let mut visited = HashSet::from([object_identity(&pages)]);
            if !self.insert_delete_pdf_page(&pages, index, Some(page), true, &mut visited) {
                return false;
            }
        }
        self.page_list
            .insert(index as usize, page.borrow().obj_num());
        true
    }

    /// Return the removed page's object number, retaining the actual indirect
    /// object until the caller explicitly invokes `set_page_to_null_object`.
    pub fn delete_page(&mut self, index: i32) -> u32 {
        let Some(pages) = self.pages_dict() else {
            return 0;
        };
        let count = with_dict(&pages, |dict| dict.get_integer_for(b"Count", 0)).unwrap();
        if index < 0 || index >= count {
            return 0;
        }
        let Some(page) = self.page_dictionary(index as usize) else {
            return 0;
        };
        let mut visited = HashSet::from([object_identity(&pages)]);
        if !self.insert_delete_pdf_page(&pages, index, None, false, &mut visited) {
            return 0;
        }
        self.page_list.remove(index as usize);
        let number = page.borrow().obj_num();
        number
    }

    pub fn set_page_to_null_object(&mut self, number: u32) {
        if number == 0 || self.page_list.is_empty() {
            return;
        }
        for index in 0..self.page_list.len() {
            self.page_dictionary(index);
        }
        if self.page_list.contains(&number) {
            return;
        }
        let holder = self.holder();
        holder.borrow_mut().delete_indirect_object(number);
        assert!(holder
            .borrow_mut()
            .replace_indirect_object_if_higher_generation(
                number,
                Some(super::cpdf_object::retain_object(CpdfScalarObject::null())),
            ));
    }

    /// Move the selected pages, in the caller's order, to an index in the
    /// remaining page list. Source: CPDF_Document::MovePages (ordinary PDF).
    pub fn move_pages(&mut self, indices: &[i32], destination: i32) -> bool {
        let count = self
            .pages_dict()
            .and_then(|pages| with_dict(&pages, |dict| dict.get_integer_for(b"Count", 0)))
            .unwrap_or(0);
        if count <= 0
            || indices.is_empty()
            || indices.len() > count as usize
            || destination < 0
            || destination as usize > count as usize - indices.len()
        {
            return false;
        }
        let mut unique = HashSet::with_capacity(indices.len());
        let mut pages = Vec::with_capacity(indices.len());
        for &index in indices {
            if index < 0 || !unique.insert(index) {
                return false;
            }
            let Some(page) = self.page_dictionary(index as usize) else {
                return false;
            };
            pages.push(page);
        }
        let mut deletions: Vec<_> = unique.into_iter().collect();
        deletions.sort_unstable_by(|a, b| b.cmp(a));
        for index in deletions {
            self.delete_page(index);
        }
        for (offset, page) in pages.into_iter().enumerate() {
            if !self.insert_new_page(destination + offset as i32, &page) {
                return false;
            }
        }
        true
    }
    pub fn has_valid_cross_reference_table(&self) -> bool {
        self.has_valid_cross_reference_table
    }
    pub fn page_count(&self) -> usize {
        self.page_list.len()
    }
    pub fn page_index(&mut self, obj_num: u32) -> Option<usize> {
        let mut skip_count = 0usize;
        let mut skipped = false;
        for (index, &number) in self.page_list.iter().enumerate() {
            if number == obj_num {
                return Some(index);
            }
            if !skipped && number == 0 {
                skip_count = index;
                skipped = true;
            }
        }
        let pages = self.pages_dict()?;
        let found = find_page_index(&pages, &mut skip_count, obj_num, &mut 0, 0)?;
        if found >= self.page_list.len() {
            return None;
        }
        let page = CpdfIndirectObjectHolder::get_or_parse_indirect_object(&self.holder(), obj_num);
        if page.as_ref().is_some_and(Self::is_valid_page_object) {
            self.page_list[found] = obj_num;
        }
        Some(found)
    }
    pub(super) fn page_obj_num_for_availability(&self, index: usize) -> Option<u32> {
        self.page_list
            .get(index)
            .copied()
            .filter(|&number| number != 0)
    }
    pub fn is_page_loaded(&self, index: usize) -> bool {
        self.page_list.get(index).is_some_and(|&num| num != 0)
    }
    pub fn set_page_obj_num(&mut self, index: usize, obj_num: u32) {
        self.page_list[index] = obj_num;
    }
    pub fn parsed_page_count(&self) -> u32 {
        self.page_data.parsed_page_count()
    }
    pub fn increment_parsed_page_count(&mut self) {
        self.page_data.increment_parsed_page_count();
    }
    pub fn user_permissions(&self, owner: bool) -> u32 {
        self.parser.permissions(owner)
    }
    pub fn file_identifier(&self) -> Option<ObjectRef> {
        self.parser.id_array()
    }

    pub fn load_doc(&mut self, file: Rc<dyn SeekableReadStream>, password: &[u8]) -> ParserError {
        self.reset_optional_content();
        let error = self.parser.start_parse(file, password);
        self.handle_load_result(error)
    }

    pub fn load_linearized_doc(
        &mut self,
        validator: Rc<CpdfReadValidator>,
        password: &[u8],
    ) -> ParserError {
        self.reset_optional_content();
        let error = self.parser.start_linearized_parse(validator, password);
        self.handle_load_result(error)
    }

    fn handle_load_result(&mut self, error: ParserError) -> ParserError {
        if error == ParserError::Success {
            self.has_valid_cross_reference_table = !self.parser.xref_table_rebuilt();
            self.root_dict = self.parser.root();
            self.page_data
                .bind_document_resources(&self.parser.holder(), self.root_dict.as_ref());
            self.load_pages();
        }
        error
    }

    fn retrieve_page_count_from_root(root: &ObjectRef) -> usize {
        let Some(pages) = with_dict(root, |dict| dict.get_dict_for(b"Pages")).flatten() else {
            return 0;
        };
        if !with_dict(&pages, |dict| dict.key_exist(b"Kids")).unwrap_or(false) {
            return 1;
        }
        let mut visited = HashSet::from([object_identity(&pages)]);
        count_pages(&pages, &mut visited).unwrap_or(0)
    }

    fn pages_dict(&self) -> Option<ObjectRef> {
        with_dict(self.root_dict.as_ref()?, |dict| dict.get_dict_for(b"Pages"))?
    }

    pub fn load_pages(&mut self) {
        if let Some(header) = self.parser.linearized_header() {
            let obj_num = header.first_page_obj_num();
            let object =
                CpdfIndirectObjectHolder::get_or_parse_indirect_object(&self.holder(), obj_num);
            if object.as_ref().is_some_and(Self::is_valid_page_object) {
                debug_assert!(header.first_page_no() < header.page_count());
                self.page_list.resize(header.page_count() as usize, 0);
                self.page_list[header.first_page_no() as usize] = obj_num;
                return;
            }
        }
        self.page_list.resize(
            self.root_dict
                .as_ref()
                .map_or(0, Self::retrieve_page_count_from_root),
            0,
        );
    }

    pub fn is_valid_page_object(object: &ObjectRef) -> bool {
        with_dict(object, |dict| {
            crate::fxcrt::bytestring::eq_cstr(&dict.get_name_for(b"Type"), b"Page")
        })
        .unwrap_or(false)
    }

    pub fn page_dictionary(&mut self, index: usize) -> Option<ObjectRef> {
        let &obj_num = self.page_list.get(index)?;
        if obj_num != 0 {
            let object =
                CpdfIndirectObjectHolder::get_or_parse_indirect_object(&self.holder(), obj_num);
            if let Some(object) = object {
                if object.borrow().as_any().is::<CpdfDictionary>() {
                    return Some(object);
                }
            }
        }
        let pages = self.pages_dict()?;
        if self.tree_traversal.is_empty() {
            self.reset_traversal();
            self.tree_traversal.push(PageTreeData {
                pages_dict: pages,
                child_index: 0,
            });
        }
        let mut pages_to_go = index as i32 - self.next_page_to_traverse + 1;
        let page = self.traverse_pdf_pages(index, &mut pages_to_go, 0);
        self.next_page_to_traverse = index as i32 + 1;
        page
    }

    fn traverse_pdf_pages(
        &mut self,
        target: usize,
        pages_to_go: &mut i32,
        level: usize,
    ) -> Option<ObjectRef> {
        if *pages_to_go < 0 || self.reached_max_page_level {
            return None;
        }
        let pages = self.tree_traversal.get(level)?.pages_dict.clone();
        let kids = with_dict(&pages, |dict| dict.get_array_for(b"Kids"))?;
        let Some(kids) = kids else {
            self.tree_traversal.pop();
            if *pages_to_go != 1 || node_type(&pages)? == NodeType::Branch {
                return None;
            }
            self.page_list[target] = pages.borrow().obj_num();
            return Some(pages);
        };
        if level >= MAX_PAGE_LEVEL {
            self.tree_traversal.pop();
            self.reached_max_page_level = true;
            return None;
        }
        let len = with_array(&kids, CpdfArray::size)?;
        let mut page = None;
        let mut index = self.tree_traversal[level].child_index;
        while index < len {
            if *pages_to_go == 0 {
                break;
            }
            {
                let mut borrowed = kids.borrow_mut();
                borrowed
                    .as_any_mut()
                    .downcast_mut::<CpdfArray>()?
                    .convert_to_indirect_object_at(index, &self.holder());
            }
            let kid = with_array(&kids, |array| array.get_dict_at(index))?;
            let Some(kid) = kid else {
                *pages_to_go -= 1;
                self.tree_traversal[level].child_index += 1;
                index += 1;
                continue;
            };
            if Rc::ptr_eq(&kid, &pages) {
                self.tree_traversal[level].child_index += 1;
                index += 1;
                continue;
            }
            if !with_dict(&kid, |dict| dict.key_exist(b"Kids"))? {
                let page_index = target as i32 - *pages_to_go + 1;
                if let Some(slot) = self.page_list.get_mut(page_index as usize) {
                    *slot = kid.borrow().obj_num();
                }
                *pages_to_go -= 1;
                self.tree_traversal[level].child_index += 1;
                if *pages_to_go == 0 {
                    page = Some(kid);
                    break;
                }
            } else {
                if self.tree_traversal.len() == level + 1 {
                    self.tree_traversal.push(PageTreeData {
                        pages_dict: kid,
                        child_index: 0,
                    });
                }
                let child = self.traverse_pdf_pages(target, pages_to_go, level + 1);
                if self.tree_traversal.len() == level + 1 {
                    self.tree_traversal[level].child_index += 1;
                }
                if self.tree_traversal.len() != level + 1
                    || *pages_to_go == 0
                    || self.reached_max_page_level
                {
                    page = child;
                    break;
                }
            }
            index += 1;
        }
        if self
            .tree_traversal
            .get(level)
            .is_some_and(|data| data.child_index == len)
        {
            self.tree_traversal.pop();
        }
        page
    }

    fn reset_traversal(&mut self) {
        self.next_page_to_traverse = 0;
        self.reached_max_page_level = false;
        self.tree_traversal.clear();
    }

    pub fn info(&mut self) -> Option<ObjectRef> {
        if let Some(info) = &self.info_dict {
            return Some(info.clone());
        }
        let obj_num = self.parser.info_obj_num();
        let info = CpdfIndirectObjectHolder::get_or_parse_indirect_object(&self.holder(), obj_num)
            .filter(|object| object.borrow().as_any().is::<CpdfDictionary>());
        self.info_dict = info.clone();
        info
    }
}

#[cfg(test)]
#[path = "cpdf_document_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "cpdf_document_edit_tests.rs"]
mod edit_tests;
