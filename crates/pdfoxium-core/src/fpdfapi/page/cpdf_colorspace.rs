// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Common color-space behavior from `core/fpdfapi/page/cpdf_colorspace.{h,cpp}`.
//! The shared source families are split into calibrated, ICCBased, and spot
//! modules; base-space loading distinguishes native direct and cached calls.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use super::cpdf_calibratedcs::{CpdfCalGray, CpdfCalRgb, CpdfLabCs};
use super::cpdf_devicecs::CpdfDeviceCs;
use super::cpdf_iccbasedcs::CpdfIccBasedCs;
use super::cpdf_iccprofile::CpdfIccProfile;
use super::cpdf_indexedcs::CpdfIndexedCs;
use super::cpdf_pattern::PatternRef;
use super::cpdf_patterncs::CpdfPatternCs;
use super::cpdf_spotcs::{CpdfDeviceNCs, CpdfSeparationCs};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxge::dib::fx_dib::FxRgbStruct;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ColorSpaceFamily {
    Unknown = 0,
    DeviceGray = 1,
    DeviceRgb = 2,
    DeviceCmyk = 3,
    CalGray = 4,
    CalRgb = 5,
    Lab = 6,
    IccBased = 7,
    Separation = 8,
    DeviceN = 9,
    Indexed = 10,
    Pattern = 11,
}

pub type ColorSpaceRef = Rc<dyn CpdfColorSpace>;

thread_local! {
    // Only CMYK conversion arithmetic observes the shared standard-conversion
    // counter. A nonwrapping revision conservatively invalidates image proofs
    // across balanced changes as well as paused/reentrant conversion changes.
    static CMYK_CONVERSION_REVISION: Cell<Option<u64>> = const { Cell::new(Some(0)) };
}
pub(crate) fn cmyk_conversion_revision() -> Option<u64> {
    CMYK_CONVERSION_REVISION.with(Cell::get)
}

pub const MAX_PATTERN_COLOR_COMPS: usize = 16;

#[derive(Clone)]
pub struct PatternValue {
    pattern: Option<PatternRef>,
    comps: [f32; MAX_PATTERN_COLOR_COMPS],
}

impl Default for PatternValue {
    fn default() -> Self {
        Self {
            pattern: None,
            comps: [0.0; MAX_PATTERN_COLOR_COMPS],
        }
    }
}

impl PatternValue {
    pub fn set_comps(&mut self, comps: &[f32]) {
        assert!(comps.len() <= MAX_PATTERN_COLOR_COMPS);
        self.comps[..comps.len()].copy_from_slice(comps);
    }
    pub fn comps(&self) -> &[f32] {
        &self.comps
    }
    pub fn pattern(&self) -> Option<PatternRef> {
        self.pattern.clone()
    }
    pub fn set_pattern(&mut self, pattern: Option<PatternRef>) {
        self.pattern = pattern;
    }
}

pub struct CpdfColorSpaceData {
    array: OnceCell<ObjectRef>,
    family: ColorSpaceFamily,
    components: u32,
    std_conversion: Cell<u32>,
}

impl CpdfColorSpaceData {
    pub fn new(family: ColorSpaceFamily, components: u32) -> Self {
        Self {
            array: OnceCell::new(),
            family,
            components,
            std_conversion: Cell::new(0),
        }
    }

    pub fn family(&self) -> ColorSpaceFamily {
        self.family
    }

    pub fn component_count(&self) -> u32 {
        self.components
    }

    pub fn set_component_count(&mut self, count: u32) {
        self.components = count;
    }

    pub fn enable_std_conversion(&self, enabled: bool) {
        let count = self.std_conversion.get();
        if enabled {
            self.std_conversion.set(count.wrapping_add(1));
        } else if count != 0 {
            self.std_conversion.set(count - 1);
        }
        if self.family == ColorSpaceFamily::DeviceCmyk && self.std_conversion.get() != count {
            CMYK_CONVERSION_REVISION.with(|revision| {
                revision.set(revision.get().and_then(|value| value.checked_add(1)));
            });
        }
    }

    pub fn is_std_conversion_enabled(&self) -> bool {
        self.std_conversion.get() != 0
    }
}

pub trait CpdfColorSpace {
    fn data(&self) -> &CpdfColorSpaceData;
    fn get_rgb(&self, components: &[f32]) -> Option<FxRgbStruct<f32>>;

    fn as_indexed_cs(&self) -> Option<&CpdfIndexedCs> {
        None
    }

    fn get_icc_profile(&self) -> Option<&CpdfIccProfile> {
        None
    }

    fn as_pattern_cs(&self) -> Option<&CpdfPatternCs> {
        None
    }

    fn family(&self) -> ColorSpaceFamily {
        self.data().family()
    }

    fn component_count(&self) -> u32 {
        self.data().component_count()
    }

    fn is_special(&self) -> bool {
        matches!(
            self.family(),
            ColorSpaceFamily::Separation
                | ColorSpaceFamily::DeviceN
                | ColorSpaceFamily::Indexed
                | ColorSpaceFamily::Pattern
        )
    }

    fn get_rgb_or_zeros_on_error(&self, components: &[f32]) -> FxRgbStruct<f32> {
        self.get_rgb(components).unwrap_or_default()
    }

    fn get_default_value(&self, _component: usize) -> (f32, f32, f32) {
        (0.0, 0.0, 1.0)
    }

    fn create_buf_and_set_default_color(&self) -> Vec<f32> {
        assert_ne!(self.family(), ColorSpaceFamily::Pattern);
        (0..self.component_count() as usize)
            .map(|component| self.get_default_value(component).0)
            .collect()
    }

    fn translate_image_line(
        &self,
        dest: &mut [u8],
        src: &[u8],
        pixels: usize,
        _image_width: i32,
        _image_height: i32,
        trans_mask: bool,
    ) {
        assert!(!trans_mask);
        let components = self.component_count() as usize;
        assert!(src.len() >= pixels * components && dest.len() >= pixels * 3);
        let divisor = if self.family() == ColorSpaceFamily::Indexed {
            1.0
        } else {
            255.0
        };
        let mut values = vec![0.0; components];
        for (source, target) in src
            .chunks_exact(components)
            .zip(dest.chunks_exact_mut(3))
            .take(pixels)
        {
            for (value, component) in values.iter_mut().zip(source) {
                *value = f32::from(*component) / divisor;
            }
            let rgb = self.get_rgb_or_zeros_on_error(&values);
            target[0] = (rgb.blue * 255.0) as i32 as u8;
            target[1] = (rgb.green * 255.0) as i32 as u8;
            target[2] = (rgb.red * 255.0) as i32 as u8;
        }
    }

    fn enable_std_conversion(&self, enabled: bool) {
        self.data().enable_std_conversion(enabled);
    }

    fn is_normal(&self) -> bool {
        matches!(
            self.family(),
            ColorSpaceFamily::DeviceGray
                | ColorSpaceFamily::DeviceRgb
                | ColorSpaceFamily::DeviceCmyk
                | ColorSpaceFamily::CalGray
                | ColorSpaceFamily::CalRgb
        )
    }
}

pub fn components_for_family(family: ColorSpaceFamily) -> u32 {
    match family {
        ColorSpaceFamily::DeviceGray => 1,
        ColorSpaceFamily::DeviceRgb => 3,
        ColorSpaceFamily::DeviceCmyk => 4,
        _ => panic!("components_for_family applies only to device color spaces"),
    }
}

thread_local! {
    static STOCK_GRAY: RefCell<Option<ColorSpaceRef>> = const { RefCell::new(None) };
    static STOCK_RGB: RefCell<Option<ColorSpaceRef>> = const { RefCell::new(None) };
    static STOCK_CMYK: RefCell<Option<ColorSpaceRef>> = const { RefCell::new(None) };
    static STOCK_PATTERN: RefCell<Option<ColorSpaceRef>> = const { RefCell::new(None) };
}

fn stock_space(
    slot: &'static std::thread::LocalKey<RefCell<Option<ColorSpaceRef>>>,
    make: impl FnOnce() -> ColorSpaceRef,
) -> ColorSpaceRef {
    slot.with(|slot| Rc::clone(slot.borrow_mut().get_or_insert_with(make)))
}

pub fn destroy_stock_color_spaces() {
    STOCK_GRAY.with(|slot| *slot.borrow_mut() = None);
    STOCK_RGB.with(|slot| *slot.borrow_mut() = None);
    STOCK_CMYK.with(|slot| *slot.borrow_mut() = None);
    STOCK_PATTERN.with(|slot| *slot.borrow_mut() = None);
}

pub fn get_stock_device_cs(family: ColorSpaceFamily) -> ColorSpaceRef {
    match family {
        ColorSpaceFamily::DeviceGray => stock_space(&STOCK_GRAY, || {
            Rc::new(CpdfDeviceCs::new(ColorSpaceFamily::DeviceGray))
        }),
        ColorSpaceFamily::DeviceRgb => stock_space(&STOCK_RGB, || {
            Rc::new(CpdfDeviceCs::new(ColorSpaceFamily::DeviceRgb))
        }),
        ColorSpaceFamily::DeviceCmyk => stock_space(&STOCK_CMYK, || {
            Rc::new(CpdfDeviceCs::new(ColorSpaceFamily::DeviceCmyk))
        }),
        _ => panic!("stock device color space requires a device family"),
    }
}

pub fn get_stock_cs_for_name(name: &[u8]) -> Option<ColorSpaceRef> {
    let name = crate::fxcrt::bytestring::c_str(name);
    if name == b"Pattern" {
        return Some(stock_space(&STOCK_PATTERN, || {
            Rc::new(CpdfPatternCs::new_stock())
        }));
    }
    let family = match name {
        b"DeviceRGB" | b"RGB" => ColorSpaceFamily::DeviceRgb,
        b"DeviceGray" | b"G" => ColorSpaceFamily::DeviceGray,
        b"DeviceCMYK" | b"CMYK" => ColorSpaceFamily::DeviceCmyk,
        _ => return None,
    };
    Some(get_stock_device_cs(family))
}

/// Loads the calibrated families handled within `cpdf_colorspace.cpp`.
fn allocated_family(name: &[u8]) -> Option<&'static [u8]> {
    let mut id = [0; 4];
    let count = name.len().min(4);
    id[..count].copy_from_slice(&name[..count]);
    match &id {
        b"CalG" => Some(b"CalGray"),
        b"CalR" => Some(b"CalRGB"),
        b"Lab\0" => Some(b"Lab"),
        b"ICCB" => Some(b"ICCBased"),
        b"Inde" | b"I\0\0\0" => Some(b"Indexed"),
        b"Sepa" => Some(b"Separation"),
        b"Devi" => Some(b"DeviceN"),
        b"Patt" => Some(b"Pattern"),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub(crate) enum BaseSpaceLoad {
    Direct,
    DocumentCache,
}

pub fn load_calibrated_cs(array: &CpdfArray) -> Option<ColorSpaceRef> {
    if array.size() < 2 {
        return None;
    }
    let family = array.get_direct_object_at(0)?.borrow().get_string();
    match allocated_family(&family)? {
        b"CalGray" => Some(Rc::new(CpdfCalGray::load(array)?)),
        b"CalRGB" => Some(Rc::new(CpdfCalRgb::load(array)?)),
        b"Lab" => Some(Rc::new(CpdfLabCs::load(array)?)),
        _ => None,
    }
}

/// Standalone loader with PDFium's direct-load recursion guard.
pub fn load_supported_cs(object: &ObjectRef) -> Option<ColorSpaceRef> {
    load_supported_cs_guarded(object, &mut HashSet::new())
}

fn load_supported_cs_guarded(
    object: &ObjectRef,
    visited: &mut HashSet<usize>,
) -> Option<ColorSpaceRef> {
    let object = get_direct_object(object)?;
    let identity = Rc::as_ptr(&object) as *const () as usize;
    if !visited.insert(identity) {
        return None;
    }
    let result = load_color_space_with_context(
        &object,
        &mut |base, mode| {
            if matches!(mode, BaseSpaceLoad::DocumentCache)
                && get_direct_object(base)?
                    .borrow()
                    .as_any()
                    .is::<CpdfStream>()
            {
                return None;
            }
            load_supported_cs_guarded(base, visited)
        },
        &mut |stream, components| CpdfIccProfile::new(stream, components).map(Rc::new),
    );
    visited.remove(&identity);
    result
}

/// Color-space allocation/v_Load with the document's guarded base resolver and
/// retained ICC cache supplied by the caller.
pub(crate) fn load_color_space_with_context(
    object: &ObjectRef,
    load_base: &mut dyn FnMut(&ObjectRef, BaseSpaceLoad) -> Option<ColorSpaceRef>,
    load_profile: &mut dyn FnMut(&ObjectRef, u32) -> Option<Rc<CpdfIccProfile>>,
) -> Option<ColorSpaceRef> {
    let object = get_direct_object(object)?;

    let object_type = object.borrow().object_type();
    match object_type {
        ObjectType::Name => return get_stock_cs_for_name(&object.borrow().get_string()),
        ObjectType::Stream => {
            let borrowed = object.borrow();
            let stream = borrowed.as_any().downcast_ref::<CpdfStream>()?;
            let dict = stream.get_dict();
            let borrowed = dict.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            return dict.with_locked_entries(|entries| {
                entries.values().find_map(|value| {
                    (value.borrow().object_type() == ObjectType::Name)
                        .then(|| get_stock_cs_for_name(&value.borrow().get_string()))
                        .flatten()
                })
            });
        }
        ObjectType::Array => {}
        _ => return None,
    }
    let family = {
        let borrowed = object.borrow();
        let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
        if array.is_empty() {
            return None;
        }
        let family = array.get_direct_object_at(0)?.borrow().get_string();
        if array.size() == 1 {
            return get_stock_cs_for_name(&family);
        }
        family
    };
    let loaded: Option<ColorSpaceRef> = match allocated_family(&family)? {
        b"CalGray" | b"CalRGB" | b"Lab" => {
            let borrowed = object.borrow();
            load_calibrated_cs(borrowed.as_any().downcast_ref::<CpdfArray>()?)
        }
        b"Indexed" | b"I" => Some(Rc::new(CpdfIndexedCs::load(&object, |base| {
            load_base(base, BaseSpaceLoad::DocumentCache)
        })?)),
        b"ICCBased" => Some(Rc::new(CpdfIccBasedCs::load(
            &object,
            |base| load_base(base, BaseSpaceLoad::Direct),
            load_profile,
        )?)),
        b"Pattern" => Some(Rc::new(CpdfPatternCs::load(&object, |base| {
            load_base(base, BaseSpaceLoad::DocumentCache)
        })?)),
        b"Separation" => Some(Rc::new(CpdfSeparationCs::load(&object, |base| {
            load_base(base, BaseSpaceLoad::Direct)
        })?)),
        b"DeviceN" => Some(Rc::new(CpdfDeviceNCs::load(&object, |base| {
            load_base(base, BaseSpaceLoad::Direct)
        })?)),
        _ => None,
    };
    if let Some(space) = &loaded {
        let _ = space.data().array.set(Rc::clone(&object));
    }
    loaded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn direct_alternates_and_cached_bases_keep_native_stream_rules() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        let stream = retain_object(CpdfStream::from_data_and_dict(
            Vec::new(),
            retain_object(dict),
        ));
        let number = objects.borrow_mut().add_indirect_object(stream);
        let mut separation = CpdfArray::new();
        separation.append_new_name(b"SeparationSuffix");
        separation.append_new_name(b"Ink");
        separation.append_new(CpdfReference::new(&objects, number));
        let data = CpdfDocPageData::new();
        let cs = data
            .get_color_space(&retain_object(separation), None)
            .unwrap();
        assert_eq!(cs.family(), ColorSpaceFamily::Separation);
        assert_eq!(cs.get_rgb(&[0.25]).unwrap().red, 0.25);
        let mut indexed = CpdfArray::new();
        indexed.append_new_name(b"IndexedSuffix");
        indexed.append_new(CpdfReference::new(&objects, number));
        indexed.append_new(CpdfScalarObject::number_from_signed(0));
        indexed.append_new(crate::fpdfapi::parser::cpdf_string::CpdfString::from_bytes(
            None,
            &[0, 0, 0],
        ));
        assert!(data
            .get_color_space(&retain_object(indexed), None)
            .is_none());
    }

    #[test]
    fn separation_none_stops_before_resolving_alternates_and_retains_its_array() {
        let objects = CpdfIndirectObjectHolder::new();
        let reads = Rc::new(Cell::new(0));
        let seen = Rc::clone(&reads);
        objects.borrow_mut().set_parser(Some(Rc::new(move |_| {
            seen.set(seen.get() + 1);
            None
        })));
        let mut array = CpdfArray::new();
        array.append_new_name(b"Separation");
        array.append_new_name(b"None");
        array.append_new(CpdfReference::new(&objects, 100));
        array.append_new(CpdfReference::new(&objects, 101));
        let array = retain_object(array);
        let weak = Rc::downgrade(&array);
        let cs = load_supported_cs(&array).unwrap();
        assert_eq!(reads.get(), 0);
        drop(array);
        assert!(weak.upgrade().is_some());
        drop(cs);
        assert!(weak.upgrade().is_none());
    }
}
