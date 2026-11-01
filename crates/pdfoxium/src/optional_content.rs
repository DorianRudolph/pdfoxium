// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Optional content groups (PDF 1.7 §4.10). Visibility is transient; explicitly
//! writing a configuration is a document edit. IDs belong to one open document.
use crate::{Document, Error, Result};
use pdfoxium_core::fpdfapi::page::{
    cpdf_occontext::UsageType,
    optional_content::{OcEnvironment, OcOrder, OcSnapshot, OcState},
};
use std::sync::Arc;

pub use pdfoxium_core::fpdfapi::page::optional_content::OcEnvironment as UsageEnvironment;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct OptionalContentGroupId {
    owner: u64,
    object: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct OptionalContentConfigurationId {
    owner: u64,
    index: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OptionalContentGroup {
    pub id: OptionalContentGroupId,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub intents: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OptionalContentConfiguration {
    pub id: OptionalContentConfigurationId,
    pub name: String,
    pub creator: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum OptionalContentOrder {
    Group {
        id: OptionalContentGroupId,
        children: Vec<Self>,
    },
    Label {
        text: String,
        children: Vec<Self>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionalContentEvent {
    View,
    Print,
    Export,
}
impl OptionalContentEvent {
    fn core(self) -> UsageType {
        match self {
            Self::View => UsageType::View,
            Self::Print => UsageType::Print,
            Self::Export => UsageType::Export,
        }
    }
}
/// Immutable Send + Sync render state. A snapshot can be used with its document,
/// or worker documents opened from the *same Arc of source bytes*. Equal-looking
/// catalogs in unrelated documents do not make snapshots interchangeable.
#[derive(Clone, Debug)]
pub struct OptionalContentSnapshot {
    owner: u64,
    catalog_generation: u64,
    source: Option<Arc<[u8]>>,
    pub(crate) state: Option<OcSnapshot>,
}
impl PartialEq for OptionalContentSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner
            && self.catalog_generation == other.catalog_generation
            && self.state == other.state
    }
}
/// Independent, Send viewing context. Editing this does not mutate a PDF.
#[derive(Clone, Debug)]
pub struct OptionalContentState {
    owner: u64,
    catalog_generation: u64,
    source: Option<Arc<[u8]>>,
    pub(crate) state: Option<OcState>,
}
#[derive(Clone)]
pub struct OptionalContent {
    document: Document,
}
impl Document {
    pub fn optional_content(&self) -> Result<OptionalContent> {
        self.with_core(|_| Ok(()))?;
        Ok(OptionalContent {
            document: self.clone(),
        })
    }
    pub(crate) fn optional_content_snapshot(&self) -> Result<OptionalContentSnapshot> {
        self.with_core(|core| {
            let core = core.borrow();
            Ok(OptionalContentSnapshot {
                owner: self.optional_content_identity(),
                catalog_generation: core.optional_content_generation(),
                source: self.optional_content_source(),
                state: core
                    .optional_content()
                    .map(|s| s.borrow().snapshot(UsageType::View)),
            })
        })
    }
    pub fn optional_content_state(&self) -> Result<OptionalContentState> {
        self.with_core(|core| {
            Ok(OptionalContentState {
                owner: self.optional_content_identity(),
                catalog_generation: core.borrow().optional_content_generation(),
                source: self.optional_content_source(),
                state: core.borrow().optional_content().map(|s| s.borrow().clone()),
            })
        })
    }
}
impl Document {
    /// Replace the current view from a context belonging to this source. Running
    /// renders retain their original immutable snapshot.
    pub fn set_optional_content_state(&self, state: &OptionalContentState) -> Result<()> {
        state.snapshot().validate(self)?;
        self.with_form_core(|doc| {
            let Some(incoming) = &state.state else {
                return Ok(());
            };
            let current = doc
                .borrow()
                .optional_content()
                .ok_or(Error::InvalidArgument("document optional content changed"))?;
            if current.borrow().model != incoming.model {
                return Err(Error::InvalidArgument("optional content model changed"));
            }
            *current.borrow_mut() = incoming.clone();
            Ok(())
        })
    }
}

impl OptionalContentSnapshot {
    pub(crate) fn same_source(&self, other: &Self) -> bool {
        self.catalog_generation == other.catalog_generation
            && (self.owner == other.owner
                || self
                    .source
                    .as_ref()
                    .zip(other.source.as_ref())
                    .is_some_and(|(a, b)| Arc::ptr_eq(a, b)))
    }
    pub fn revision(&self) -> u64 {
        self.state.as_ref().map_or(0, |s| s.revision)
    }
    pub(crate) fn validate(&self, document: &Document) -> Result<()> {
        if self.catalog_generation != document.optional_content_generation()? {
            return Err(Error::InvalidArgument(
                "optional content catalog changed; create a fresh context",
            ));
        }
        if self.owner == document.optional_content_identity()
            || self
                .source
                .as_ref()
                .zip(document.optional_content_source().as_ref())
                .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
        {
            Ok(())
        } else {
            Err(Error::InvalidArgument(
                "optional content snapshot belongs to a different document/source",
            ))
        }
    }
}
impl OptionalContentState {
    pub fn is_empty(&self) -> bool {
        self.state.is_none()
    }
    pub fn groups(&self) -> Vec<OptionalContentGroup> {
        self.state.as_ref().map_or_else(Vec::new, |s| {
            s.model
                .groups
                .iter()
                .enumerate()
                .map(|(i, g)| OptionalContentGroup {
                    id: OptionalContentGroupId {
                        owner: self.owner,
                        object: g.object,
                    },
                    name: g.name.clone(),
                    visible: s.visible(i).unwrap_or(true),
                    locked: s.model.configurations[s.configuration].locked.contains(&i),
                    intents: g
                        .intents
                        .iter()
                        .map(|x| String::from_utf8_lossy(x).into_owned())
                        .collect(),
                })
                .collect()
        })
    }
    pub fn configurations(&self) -> Vec<OptionalContentConfiguration> {
        self.state.as_ref().map_or_else(Vec::new, |s| {
            s.model
                .configurations
                .iter()
                .enumerate()
                .map(|(index, c)| OptionalContentConfiguration {
                    id: OptionalContentConfigurationId {
                        owner: self.owner,
                        index,
                    },
                    name: c.name.clone(),
                    creator: c.creator.clone(),
                })
                .collect()
        })
    }
    pub fn configuration(&self) -> Option<OptionalContentConfigurationId> {
        self.state.as_ref().map(|s| OptionalContentConfigurationId {
            owner: self.owner,
            index: s.configuration,
        })
    }
    pub fn order(&self) -> Vec<OptionalContentOrder> {
        fn convert(order: &[OcOrder], s: &OptionalContentState) -> Vec<OptionalContentOrder> {
            order
                .iter()
                .map(|n| match n {
                    OcOrder::Group { index, children } => OptionalContentOrder::Group {
                        id: OptionalContentGroupId {
                            owner: s.owner,
                            object: s.state.as_ref().unwrap().model.groups[*index].object,
                        },
                        children: convert(children, s),
                    },
                    OcOrder::Label { text, children } => OptionalContentOrder::Label {
                        text: text.clone(),
                        children: convert(children, s),
                    },
                })
                .collect()
        }
        self.state.as_ref().map_or_else(Vec::new, |s| {
            convert(&s.model.configurations[s.configuration].order, self)
        })
    }
    pub fn environment(&self) -> UsageEnvironment {
        self.state
            .as_ref()
            .map_or_else(OcEnvironment::default, |s| s.environment.clone())
    }
    pub fn set_environment(&mut self, environment: UsageEnvironment) -> Result<bool> {
        match &mut self.state {
            Some(s) => s
                .set_environment(environment)
                .map_err(Error::InvalidArgument),
            None => Ok(false),
        }
    }
    fn index(&self, id: OptionalContentGroupId) -> Result<usize> {
        if id.owner != self.owner {
            return Err(Error::InvalidArgument(
                "optional content ID belongs to another document",
            ));
        }
        self.state
            .as_ref()
            .and_then(|s| s.model.groups.iter().position(|g| g.object == id.object))
            .ok_or(Error::InvalidArgument(
                "optional content group no longer exists",
            ))
    }
    /// Interactive change: honor Locked, radio groups and automatic-state overrides.
    pub fn set_visible(&mut self, id: OptionalContentGroupId, visible: bool) -> Result<bool> {
        self.change(id, visible, true, true)
    }
    /// Explicit programmatic change; Locked constrains only the viewer UI.
    pub fn set_visible_programmatic(
        &mut self,
        id: OptionalContentGroupId,
        visible: bool,
        preserve_radio: bool,
    ) -> Result<bool> {
        self.change(id, visible, false, preserve_radio)
    }
    fn change(
        &mut self,
        id: OptionalContentGroupId,
        on: bool,
        user: bool,
        radio: bool,
    ) -> Result<bool> {
        let index = self.index(id)?;
        self.state
            .as_mut()
            .unwrap()
            .set_visible(index, on, user, radio)
            .map_err(Error::InvalidArgument)
    }
    pub fn apply_configuration(&mut self, id: OptionalContentConfigurationId) -> Result<()> {
        if id.owner != self.owner {
            return Err(Error::InvalidArgument(
                "optional content configuration belongs to another document",
            ));
        }
        self.state
            .as_mut()
            .ok_or(Error::InvalidArgument("document has no optional content"))?
            .apply_configuration(id.index)
            .map_err(Error::InvalidArgument)
    }
    pub fn apply_action(&mut self, action: &crate::OptionalContentAction) -> Result<bool> {
        self.state
            .as_mut()
            .ok_or(Error::InvalidArgument("document has no optional content"))?
            .apply_action(action)
            .map_err(Error::InvalidArgument)
    }
    pub fn reset(&mut self) {
        if let Some(s) = &mut self.state {
            s.reset()
        }
    }
    pub fn snapshot(&self) -> OptionalContentSnapshot {
        self.snapshot_for(OptionalContentEvent::View)
    }
    pub fn snapshot_for(&self, event: OptionalContentEvent) -> OptionalContentSnapshot {
        OptionalContentSnapshot {
            owner: self.owner,
            catalog_generation: self.catalog_generation,
            source: self.source.clone(),
            state: self.state.as_ref().map(|s| s.snapshot(event.core())),
        }
    }
}
impl OptionalContent {
    pub fn state(&self) -> Result<OptionalContentState> {
        self.document.optional_content_state()
    }
    pub fn groups(&self) -> Result<Vec<OptionalContentGroup>> {
        Ok(self.state()?.groups())
    }
    pub fn configurations(&self) -> Result<Vec<OptionalContentConfiguration>> {
        Ok(self.state()?.configurations())
    }
    pub fn snapshot(&self) -> Result<OptionalContentSnapshot> {
        self.document.optional_content_snapshot()
    }
    fn mutate<T>(&self, f: impl FnOnce(&mut OcState) -> Result<T>) -> Result<T> {
        self.document.with_form_core(|doc| {
            let state = doc
                .borrow()
                .optional_content()
                .ok_or(Error::InvalidArgument("document has no optional content"))?;
            let result = f(&mut state.borrow_mut());
            result
        })
    }
    pub fn set_visible(&self, id: OptionalContentGroupId, on: bool) -> Result<bool> {
        let index = self.state()?.index(id)?;
        self.mutate(|s| {
            s.set_visible(index, on, true, true)
                .map_err(Error::InvalidArgument)
        })
    }
    pub fn set_visible_programmatic(
        &self,
        id: OptionalContentGroupId,
        on: bool,
        preserve_radio: bool,
    ) -> Result<bool> {
        let index = self.state()?.index(id)?;
        self.mutate(|s| {
            s.set_visible(index, on, false, preserve_radio)
                .map_err(Error::InvalidArgument)
        })
    }
    pub fn apply_configuration(&self, id: OptionalContentConfigurationId) -> Result<()> {
        if id.owner != self.document.optional_content_identity() {
            return Err(Error::InvalidArgument(
                "optional content configuration belongs to another document",
            ));
        }
        self.mutate(|s| {
            s.apply_configuration(id.index)
                .map_err(Error::InvalidArgument)
        })
    }
    pub fn reset(&self) -> Result<()> {
        self.mutate(|s| {
            s.reset();
            Ok(())
        })
    }
    pub fn set_environment(&self, environment: UsageEnvironment) -> Result<bool> {
        self.mutate(|s| {
            s.set_environment(environment)
                .map_err(Error::InvalidArgument)
        })
    }
}

use pdfoxium_core::fpdfapi::parser::{
    cpdf_array::CpdfArray,
    cpdf_dictionary::CpdfDictionary,
    cpdf_document::CpdfDocument,
    cpdf_object::{retain_object, ObjectRef},
    cpdf_reference::CpdfReference,
    cpdf_string::CpdfString,
};
fn dictionary<T>(o: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> T) -> T {
    f(o.borrow().as_dictionary().expect("validated dictionary"))
}
fn dictionary_mut<T>(o: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> T) -> T {
    f(o.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("validated dictionary"))
}
fn ensure_dictionary(o: &ObjectRef, key: &[u8]) -> ObjectRef {
    let existing = dictionary(o, |d| d.get_dict_for(key));
    existing.unwrap_or_else(|| dictionary_mut(o, |d| d.set_new_dictionary_for(key)))
}
fn ensure_array(o: &ObjectRef, key: &[u8]) -> ObjectRef {
    let existing = dictionary(o, |d| d.get_array_for(key));
    existing.unwrap_or_else(|| dictionary_mut(o, |d| d.set_new_array_for(key)))
}
fn append_reference(array: &ObjectRef, doc: &CpdfDocument, object: u32) {
    array
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfArray>()
        .unwrap()
        .append_new(CpdfReference::new(&doc.holder(), object));
}
fn pdf_text(value: &str) -> CpdfString {
    let mut bytes = vec![0xfe, 0xff];
    for c in value.encode_utf16() {
        bytes.extend_from_slice(&c.to_be_bytes());
    }
    CpdfString::from_bytes(None, &bytes)
}
impl OptionalContent {
    /// Create and register a visible group, and append it to the default layer
    /// order. This is an explicit PDF edit; existing group IDs remain valid.
    pub fn create_group(&self, name: &str) -> Result<OptionalContentGroupId> {
        if name.is_empty() {
            return Err(Error::InvalidArgument("layer name is empty"));
        }
        let version = self.document.version()?;
        self.document.with_structure_edit(|doc| {
            let core = doc.borrow();
            let root = core.root().ok_or(Error::Format)?;
            if version.major == 1 && version.minor < 5 {
                dictionary_mut(&root, |d| d.set_new_name_for(b"Version", b"1.5"));
            }
            let props = ensure_dictionary(&root, b"OCProperties");
            let group = core.holder().borrow_mut().new_indirect_dictionary();
            dictionary_mut(&group, |d| {
                d.set_new_name_for(b"Type", b"OCG");
                d.set_new_for(b"Name", pdf_text(name));
            });
            let object = group.borrow().obj_num();
            append_reference(&ensure_array(&props, b"OCGs"), &core, object);
            let default = ensure_dictionary(&props, b"D");
            append_reference(&ensure_array(&default, b"Order"), &core, object);
            append_reference(&ensure_array(&default, b"ON"), &core, object);
            drop(core);
            doc.borrow_mut().reset_optional_content();
            Ok(OptionalContentGroupId {
                owner: self.document.optional_content_identity(),
                object,
            })
        })
    }
    /// Persist a context as the PDF's opening configuration. View automatic-state
    /// rules are removed so reopening restores this choice; Print/Export rules,
    /// order, intent, radio and lock metadata are retained. Ordinary toggles never
    /// call this method or mark the document dirty.
    pub fn save_default(&self, state: &OptionalContentState) -> Result<()> {
        state.snapshot().validate(&self.document)?;
        let incoming = state
            .state
            .as_ref()
            .ok_or(Error::InvalidArgument("document has no optional content"))?;
        self.document.with_structure_edit(|doc| {
            let core = doc.borrow();
            let current = core
                .optional_content()
                .ok_or(Error::InvalidArgument("document has no optional content"))?;
            if current.borrow().model != incoming.model {
                return Err(Error::InvalidArgument("optional content model changed"));
            }
            let props = dictionary(&core.root().ok_or(Error::Format)?, |d| {
                d.get_dict_for(b"OCProperties")
            })
            .ok_or(Error::Format)?;
            let original = if incoming.configuration == 0 {
                dictionary(&props, |d| d.get_dict_for(b"D"))
            } else {
                dictionary(&props, |d| d.get_array_for(b"Configs")).and_then(|a| {
                    a.borrow()
                        .as_any()
                        .downcast_ref::<CpdfArray>()
                        .and_then(|a| a.get_direct_object_at(incoming.configuration - 1))
                })
            }
            .ok_or(Error::Format)?;
            let config = original.borrow().clone_object();
            dictionary_mut(&config, |d| {
                d.set_new_name_for(b"BaseState", b"ON");
                d.remove_for(b"ON");
                d.remove_for(b"OFF");
            });
            // Alternate configurations inherit these entries from /D.
            for key in [b"Order".as_slice(), b"RBGroups"] {
                if !dictionary(&config, |d| d.key_exist(key)) {
                    if let Some(default) = dictionary(&props, |d| d.get_dict_for(b"D")) {
                        if let Some(value) = dictionary(&default, |d| d.get_object_for(key)) {
                            dictionary_mut(&config, |d| {
                                d.set_for(key, Some(value.borrow().clone_object()))
                            });
                        }
                    }
                }
            }
            let off = ensure_array(&config, b"OFF");
            for (i, group) in incoming.model.groups.iter().enumerate() {
                if incoming.visible(i) == Some(false) {
                    append_reference(&off, &core, group.object);
                }
            }
            if let Some(applications) = dictionary(&config, |d| d.get_array_for(b"AS")) {
                let mut retained = CpdfArray::new();
                let a = applications.borrow();
                let a = a.as_any().downcast_ref::<CpdfArray>().unwrap();
                for i in 0..a.size() {
                    if let Some(value) = a
                        .get_direct_object_at(i)
                        .filter(|v| v.borrow().as_dictionary().is_some())
                    {
                        if dictionary(&value, |d| d.get_byte_string_for(b"Event", b"")) != b"View" {
                            retained.append(value.borrow().clone_object());
                        }
                    }
                }
                dictionary_mut(&config, |d| d.set_new_for(b"AS", retained));
            }
            dictionary_mut(&props, |d| d.set_for(b"D", Some(config)));
            drop(core);
            doc.borrow_mut().reset_optional_content();
            Ok(())
        })
    }
}
impl OptionalContentGroupId {
    pub(crate) fn object(self, document: &Document) -> Result<u32> {
        document.optional_content_state()?.index(self)?;
        Ok(self.object)
    }
}
impl crate::Page {
    /// Assign one OCG to an object. Other optional-content marks are replaced;
    /// non-OC marked content is retained. The named property preserves indirect
    /// OCG identity in regenerated streams and after save/reopen.
    pub fn set_object_layer(
        &self,
        id: crate::ObjectId,
        group: Option<OptionalContentGroupId>,
    ) -> Result<()> {
        let number = group.map(|g| g.object(self.document())).transpose()?;
        self.with_object_access(false, |page, _, registry| {
            let core = registry.core(self.id(), id)?;
            if !(0..page.holder().page_object_count())
                .any(|i| page.holder().page_object_id_by_index(i) == Some(core))
            {
                return Err(Error::InvalidArgument(
                    "set_object_layer requires a top-level object",
                ));
            }
            Ok(())
        })?;
        self.with_object_access(true, |page, doc, registry| {
            let properties = if let Some(number) = number {
                let resources = page
                    .holder()
                    .resources()
                    .cloned()
                    .unwrap_or_else(|| retain_object(CpdfDictionary::new()));
                // Copy inherited/shared resources before extending the property map.
                let resources = resources.borrow().clone_object();
                let properties = dictionary(&resources, |d| d.get_dict_for(b"Properties"))
                    .map(|p| p.borrow().clone_object())
                    .unwrap_or_else(|| retain_object(CpdfDictionary::new()));
                let name = format!("Layer{number}").into_bytes();
                dictionary_mut(&properties, |d| {
                    d.set_new_for(&name, CpdfReference::new(&doc.borrow().holder(), number))
                });
                dictionary_mut(&resources, |d| {
                    d.set_for(b"Properties", Some(properties.clone()))
                });
                dictionary_mut(page.holder().dict(), |d| {
                    d.set_for(b"Resources", Some(resources.clone()))
                });
                page.mutable_holder().set_resources(Some(resources));
                Some((properties, name))
            } else {
                None
            };
            // Resolve through the established object edit path below, after the
            // resource edit has released its borrows.
            let _ = registry;
            Ok(properties)
        })
        .and_then(|properties| {
            self.with_object_mut(id, |o, _| {
                let marks = o.base_mut().mutable_content_marks();
                for i in (0..marks.count_items()).rev() {
                    let item = marks.get_item(i);
                    if item.borrow().name() == b"OC" {
                        marks.remove_mark(&item);
                    }
                }
                if let Some((properties, name)) = properties {
                    marks.add_mark_with_properties_holder(b"OC", properties, &name);
                }
                o.base_mut().set_dirty(true);
                Ok(())
            })
        })
    }
}

impl OptionalContentState {
    /// Whether the layer panel lists groups from all pages. `/VisiblePages`
    /// configurations restrict presentation to groups used on the visible pages.
    pub fn lists_all_pages(&self) -> bool {
        self.state
            .as_ref()
            .is_none_or(|s| s.model.configurations[s.configuration].list_all_pages)
    }
}
impl crate::Page {
    /// Registered groups used by this page's parsed marks, forms, images and
    /// annotations, including groups referenced by membership expressions.
    pub fn optional_content_groups(&self) -> Result<Vec<OptionalContentGroupId>> {
        use pdfoxium_core::fpdfapi::page::{
            cpdf_formobject::CpdfFormObject, cpdf_imageobject::CpdfImageObject,
            cpdf_pageobjectholder::CpdfPageObjectHolder,
        };
        use std::collections::HashSet;
        fn condition(
            o: &ObjectRef,
            depth: usize,
            seen: &mut HashSet<usize>,
            groups: &mut HashSet<u32>,
        ) {
            if depth > 32 || !seen.insert(std::rc::Rc::as_ptr(o) as *const () as usize) {
                return;
            }
            let values = {
                let b = o.borrow();
                if let Some(d) = b.as_dictionary() {
                    if d.get_byte_string_for(b"Type", b"") == b"OCG" {
                        groups.insert(b.obj_num());
                        return;
                    }
                    [b"OCGs".as_slice(), b"VE"]
                        .iter()
                        .filter_map(|k| d.get_direct_object_for(k))
                        .collect::<Vec<_>>()
                } else if let Some(a) = b.as_any().downcast_ref::<CpdfArray>() {
                    (0..a.size())
                        .filter_map(|i| a.get_direct_object_at(i))
                        .collect()
                } else {
                    Vec::new()
                }
            };
            for v in values {
                condition(&v, depth + 1, seen, groups);
            }
        }
        fn oc(o: &ObjectRef, seen: &mut HashSet<usize>, groups: &mut HashSet<u32>) {
            let value = o
                .borrow()
                .as_dictionary()
                .and_then(|d| d.get_dict_for(b"OC"));
            if let Some(v) = value {
                condition(&v, 0, seen, groups);
            }
        }
        fn holder(
            h: &CpdfPageObjectHolder,
            depth: usize,
            seen: &mut HashSet<usize>,
            groups: &mut HashSet<u32>,
        ) {
            if depth > 32 {
                return;
            }
            oc(h.dict(), seen, groups);
            for object in h.iter() {
                let marks = object.base().content_marks();
                for i in 0..marks.count_items() {
                    let mark = marks.get_item(i);
                    let mark = mark.borrow();
                    if mark.name() == b"OC" {
                        if let Some(v) = mark.param() {
                            condition(&v, 0, seen, groups);
                        }
                    }
                }
                if let Some(form) = object.as_any().downcast_ref::<CpdfFormObject>() {
                    holder(form.form().holder(), depth + 1, seen, groups);
                }
                if let Some(image) = object
                    .as_any()
                    .downcast_ref::<CpdfImageObject>()
                    .and_then(|i| i.image())
                {
                    if let Some(v) = image.oc() {
                        condition(&v, 0, seen, groups);
                    }
                }
            }
        }
        self.with_object_access(false, |page, doc, _| {
            let mut numbers = HashSet::new();
            let mut seen = HashSet::new();
            holder(page.holder(), 0, &mut seen, &mut numbers);
            if let Some(annots) = page.annots_array() {
                let a = annots.borrow();
                let a = a.as_any().downcast_ref::<CpdfArray>().unwrap();
                for i in 0..a.size() {
                    if let Some(o) = a.get_direct_object_at(i) {
                        oc(&o, &mut seen, &mut numbers);
                    }
                }
            }
            Ok(doc.borrow().optional_content().map_or_else(Vec::new, |s| {
                s.borrow()
                    .model
                    .groups
                    .iter()
                    .filter(|g| numbers.contains(&g.object))
                    .map(|g| OptionalContentGroupId {
                        owner: self.document().optional_content_identity(),
                        object: g.object,
                    })
                    .collect()
            }))
        })
    }
}
