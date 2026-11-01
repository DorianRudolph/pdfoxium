// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! `fpdfsdk/fpdf_sysfontinfo.cpp` / `public/fpdf_sysfontinfo.h`. Only this C
//! boundary converts opaque pointers; callbacks receive no borrowed mapper,
//! global graphics slot, or mutable Rust provider state.
use super::*;
use pdfoxium_core::{
    fxcrt::fx_codepage::{charset_from_int, FxCharset},
    fxge::{
        cfx_fontmapper::CfxFontMapper,
        cfx_fontmgr::{with_font_manager, FontMapperFactory},
        cfx_gemodule::with_ge_module,
        systemfontinfo_iface::SystemFontInfo,
    },
};
use std::{collections::HashMap, ffi::CString, rc::Weak};
pub const FXFONT_ANSI_CHARSET: c_int = 0;
pub const FXFONT_DEFAULT_CHARSET: c_int = 1;
pub const FXFONT_SYMBOL_CHARSET: c_int = 2;
pub const FXFONT_SHIFTJIS_CHARSET: c_int = 128;
pub const FXFONT_HANGEUL_CHARSET: c_int = 129;
pub const FXFONT_GB2312_CHARSET: c_int = 134;
pub const FXFONT_CHINESEBIG5_CHARSET: c_int = 136;
pub const FXFONT_GREEK_CHARSET: c_int = 161;
pub const FXFONT_VIETNAMESE_CHARSET: c_int = 163;
pub const FXFONT_HEBREW_CHARSET: c_int = 177;
pub const FXFONT_ARABIC_CHARSET: c_int = 178;
pub const FXFONT_CYRILLIC_CHARSET: c_int = 204;
pub const FXFONT_THAI_CHARSET: c_int = 222;
pub const FXFONT_EASTERNEUROPEAN_CHARSET: c_int = 238;
pub const FXFONT_FF_FIXEDPITCH: c_int = 1;
pub const FXFONT_FF_ROMAN: c_int = 1 << 4;
pub const FXFONT_FF_SCRIPT: c_int = 4 << 4;
pub const FXFONT_FW_NORMAL: c_int = 400;
pub const FXFONT_FW_BOLD: c_int = 700;

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct FPDF_SYSFONTINFO {
    pub version: c_int,
    pub Release: Option<unsafe extern "C" fn(*mut FPDF_SYSFONTINFO)>,
    pub EnumFonts: Option<unsafe extern "C" fn(*mut FPDF_SYSFONTINFO, *mut c_void)>,
    pub MapFont: Option<
        unsafe extern "C" fn(
            *mut FPDF_SYSFONTINFO,
            c_int,
            c_int,
            c_int,
            c_int,
            *const c_char,
            *mut c_int,
        ) -> *mut c_void,
    >,
    pub GetFont: Option<unsafe extern "C" fn(*mut FPDF_SYSFONTINFO, *const c_char) -> *mut c_void>,
    pub GetFontData: Option<
        unsafe extern "C" fn(*mut FPDF_SYSFONTINFO, *mut c_void, u32, *mut u8, c_ulong) -> c_ulong,
    >,
    pub GetFaceName: Option<
        unsafe extern "C" fn(*mut FPDF_SYSFONTINFO, *mut c_void, *mut c_char, c_ulong) -> c_ulong,
    >,
    pub GetFontCharset: Option<unsafe extern "C" fn(*mut FPDF_SYSFONTINFO, *mut c_void) -> c_int>,
    pub DeleteFont: Option<unsafe extern "C" fn(*mut FPDF_SYSFONTINFO, *mut c_void)>,
}
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct FPDF_CharsetFontMap {
    pub charset: c_int,
    pub fontname: *const c_char,
}
// Each entry points exclusively at immutable static, nul-terminated bytes.
unsafe impl Sync for FPDF_CharsetFontMap {}
static FONT_MAP: [FPDF_CharsetFontMap; 9] = [
    FPDF_CharsetFontMap {
        charset: 0,
        fontname: b"Helvetica\0".as_ptr().cast(),
    },
    FPDF_CharsetFontMap {
        charset: 134,
        fontname: b"SimSun\0".as_ptr().cast(),
    },
    FPDF_CharsetFontMap {
        charset: 136,
        fontname: b"MingLiU\0".as_ptr().cast(),
    },
    FPDF_CharsetFontMap {
        charset: 128,
        fontname: b"MS Gothic\0".as_ptr().cast(),
    },
    FPDF_CharsetFontMap {
        charset: 129,
        fontname: b"Batang\0".as_ptr().cast(),
    },
    FPDF_CharsetFontMap {
        charset: 204,
        fontname: b"Arial\0".as_ptr().cast(),
    },
    FPDF_CharsetFontMap {
        charset: 238,
        fontname: b"Arial\0".as_ptr().cast(),
    },
    FPDF_CharsetFontMap {
        charset: 178,
        fontname: b"Arial\0".as_ptr().cast(),
    },
    FPDF_CharsetFontMap {
        charset: -1,
        fontname: std::ptr::null(),
    },
];
struct Enumeration {
    fonts: Vec<(Vec<u8>, FxCharset)>,
}
struct ExternalOwner {
    pointer: *mut FPDF_SYSFONTINFO,
    table: FPDF_SYSFONTINFO,
    _default: Option<Rc<DefaultFontInfo>>,
}
impl Drop for ExternalOwner {
    fn drop(&mut self) {
        // Only live callback owners have registry entries. Remove the weak
        // identity before Release can reenter and install another provider.
        let _ = EXTERNAL_OWNERS.try_with(|entries| {
            entries.borrow_mut().remove(&(self.pointer as usize));
        });
        if let Some(release) = self.table.Release {
            unsafe {
                release(self.pointer);
            }
        }
    }
}
thread_local! {
    static EXTERNAL_OWNERS: RefCell<HashMap<usize, Weak<ExternalOwner>>> = RefCell::new(HashMap::new());
}
struct ExternalInfo {
    owner: Rc<ExternalOwner>,
}
fn terminated(bytes: &[u8]) -> CString {
    CString::new(bytes.split(|&byte| byte == 0).next().unwrap_or_default()).expect("nul prefix")
}
impl SystemFontInfo for ExternalInfo {
    fn enum_font_list(&mut self) -> Vec<(Vec<u8>, FxCharset)> {
        let mut entries = Enumeration { fonts: Vec::new() };
        if let Some(callback) = self.owner.table.EnumFonts {
            unsafe {
                callback(self.owner.pointer, std::ptr::addr_of_mut!(entries).cast());
            }
        }
        entries.fonts
    }
    fn map_font(
        &mut self,
        weight: i32,
        italic: bool,
        charset: FxCharset,
        pitch_family: u32,
        face: &[u8],
    ) -> Option<u64> {
        let callback = self.owner.table.MapFont?;
        let name = terminated(face);
        let mut exact = 0;
        let handle = unsafe {
            callback(
                self.owner.pointer,
                weight,
                c_int::from(italic),
                charset as c_int,
                pitch_family as c_int,
                name.as_ptr(),
                &mut exact,
            )
        };
        (!handle.is_null()).then_some(handle as usize as u64)
    }
    fn get_font(&mut self, face: &[u8]) -> Option<u64> {
        let callback = self.owner.table.GetFont?;
        let name = terminated(face);
        let handle = unsafe { callback(self.owner.pointer, name.as_ptr()) };
        (!handle.is_null()).then_some(handle as usize as u64)
    }
    fn get_font_data(&mut self, font: u64, table: u32, buffer: &mut [u8]) -> usize {
        let Some(callback) = self.owner.table.GetFontData else {
            return 0;
        };
        let pointer = if buffer.is_empty() {
            std::ptr::null_mut()
        } else {
            buffer.as_mut_ptr()
        };
        (unsafe {
            callback(
                self.owner.pointer,
                font as usize as *mut c_void,
                table,
                pointer,
                buffer.len() as c_ulong,
            )
        }) as usize
    }
    fn get_face_name(&mut self, font: u64) -> Option<Vec<u8>> {
        let callback = self.owner.table.GetFaceName?;
        let font = font as usize as *mut c_void;
        let length =
            usize::try_from(unsafe { callback(self.owner.pointer, font, std::ptr::null_mut(), 0) })
                .ok()?;
        if length == 0 {
            return None;
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).ok()?;
        bytes.resize(length, 0);
        let written = usize::try_from(unsafe {
            callback(
                self.owner.pointer,
                font,
                bytes.as_mut_ptr().cast(),
                length as c_ulong,
            )
        })
        .ok()?;
        if written > length {
            return None;
        }
        bytes.truncate(written);
        if bytes.last() == Some(&0) {
            bytes.pop();
        }
        Some(bytes)
    }
    fn get_font_charset(&mut self, font: u64) -> Option<FxCharset> {
        let callback = self.owner.table.GetFontCharset?;
        Some(charset_from_int(unsafe {
            callback(self.owner.pointer, font as usize as *mut c_void)
        }))
    }
    fn delete_font(&mut self, font: u64) {
        if let Some(callback) = self.owner.table.DeleteFont {
            unsafe {
                callback(self.owner.pointer, font as usize as *mut c_void);
            }
        }
    }
}
fn mapper_for(owner: Rc<ExternalOwner>) -> CfxFontMapper {
    let skip = owner.table.version == 2;
    let mut mapper = CfxFontMapper::new();
    mapper.set_system_font_info(Some(Box::new(ExternalInfo { owner })));
    mapper.set_skip_font_enumeration(skip);
    mapper
}
/// # Safety
/// `mapper` is the opaque token supplied to the current EnumFonts callback;
/// `face` is a valid nul-terminated byte string during this call.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_AddInstalledFont(
    mapper: *mut c_void,
    face: *const c_char,
    charset: c_int,
) {
    if mapper.is_null() || face.is_null() {
        return;
    }
    let name = unsafe { CStr::from_ptr(face) }.to_bytes().to_vec();
    unsafe {
        (*mapper.cast::<Enumeration>())
            .fonts
            .push((name, charset_from_int(charset)));
    }
}
/// # Safety
/// A non-null callback table has version1/2 layout and remains valid until its
/// Release callback. Callback/user data lifetime is the embedder's contract.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_SetSystemFontInfo(info: *mut FPDF_SYSFONTINFO) {
    if info.is_null() {
        let previous =
            with_font_manager(|manager| manager.replace_builtin_mapper(CfxFontMapper::new(), None));
        drop(previous);
        return;
    }
    let version = unsafe { (*info).version };
    if version != 1 && version != 2 {
        return;
    }
    let table = unsafe { *info };
    let existing = EXTERNAL_OWNERS.with(|entries| {
        entries
            .borrow()
            .get(&(info as usize))
            .and_then(Weak::upgrade)
    });
    let owner = existing.unwrap_or_else(|| {
        let default = DEFAULTS.with(|entries| entries.borrow().get(&(info as usize)).cloned());
        let owner = Rc::new(ExternalOwner {
            pointer: info,
            table,
            _default: default,
        });
        EXTERNAL_OWNERS.with(|entries| {
            let mut entries = entries.borrow_mut();
            entries.retain(|_, owner| owner.strong_count() != 0);
            entries.insert(info as usize, Rc::downgrade(&owner));
        });
        owner
    });
    let factory: FontMapperFactory = {
        let owner = owner.clone();
        Rc::new(move || mapper_for(owner.clone()))
    };
    let previous = with_font_manager(|manager| {
        manager.replace_builtin_mapper(mapper_for(owner), Some(factory))
    });
    drop(previous);
    // XFA initializes its installed-font catalogue from this same actual
    // mapper. Enumeration executes outside every global/mapper slot borrow.
    with_font_manager(|manager| manager.builtin_mapper_mut().load_installed_fonts());
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_GetDefaultTTFMap() -> *const FPDF_CharsetFontMap {
    FONT_MAP.as_ptr()
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_GetDefaultTTFMapCount() -> usize {
    FONT_MAP.len() - 1
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_GetDefaultTTFMapEntry(index: usize) -> *const FPDF_CharsetFontMap {
    if index < FPDF_GetDefaultTTFMapCount() {
        std::ptr::addr_of!(FONT_MAP[index])
    } else {
        std::ptr::null()
    }
}
#[repr(C)]
struct DefaultFontInfo {
    interface: FPDF_SYSFONTINFO,
    provider: RefCell<Option<Box<dyn SystemFontInfo>>>,
    installed_fonts: Vec<(Vec<u8>, FxCharset)>,
}
thread_local! {static DEFAULTS:RefCell<HashMap<usize,Rc<DefaultFontInfo>>>=RefCell::new(HashMap::new());}
unsafe fn default_owner<'a>(info: *mut FPDF_SYSFONTINFO) -> Option<&'a DefaultFontInfo> {
    unsafe { info.cast::<DefaultFontInfo>().as_ref() }
}
unsafe extern "C" fn default_release(info: *mut FPDF_SYSFONTINFO) {
    if let Some(owner) = unsafe { default_owner(info) } {
        let previous = owner.provider.borrow_mut().take();
        drop(previous);
    }
}
unsafe extern "C" fn default_enum(info: *mut FPDF_SYSFONTINFO, mapper: *mut c_void) {
    let Some(owner) = (unsafe { default_owner(info) }) else {
        return;
    };
    // GetDefaultSystemFontInfo initializes the actual provider so version 2
    // callers can map fonts before EnumFonts. Folder enumeration reports newly
    // discovered faces, so retain that initial catalogue for public enumeration.
    for (name, charset) in &owner.installed_fonts {
        let name = terminated(name);
        unsafe {
            FPDF_AddInstalledFont(mapper, name.as_ptr(), *charset as c_int);
        }
    }
}
unsafe extern "C" fn default_map(
    info: *mut FPDF_SYSFONTINFO,
    weight: c_int,
    italic: c_int,
    charset: c_int,
    pitch: c_int,
    name: *const c_char,
    _exact: *mut c_int,
) -> *mut c_void {
    let (Some(owner), Some(name)) = (
        unsafe { default_owner(info) },
        unsafe { name.as_ref() }.map(|_| unsafe { CStr::from_ptr(name) }.to_bytes()),
    ) else {
        return std::ptr::null_mut();
    };
    owner
        .provider
        .borrow_mut()
        .as_mut()
        .and_then(|provider| {
            provider.map_font(
                weight,
                italic != 0,
                charset_from_int(charset),
                pitch as u32,
                name,
            )
        })
        .map_or(std::ptr::null_mut(), |handle| {
            handle as usize as *mut c_void
        })
}
unsafe extern "C" fn default_get(info: *mut FPDF_SYSFONTINFO, name: *const c_char) -> *mut c_void {
    let Some(owner) = (unsafe { default_owner(info) }) else {
        return std::ptr::null_mut();
    };
    if name.is_null() {
        return std::ptr::null_mut();
    }
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    owner
        .provider
        .borrow_mut()
        .as_mut()
        .and_then(|provider| provider.get_font(name))
        .map_or(std::ptr::null_mut(), |handle| {
            handle as usize as *mut c_void
        })
}
unsafe extern "C" fn default_data(
    info: *mut FPDF_SYSFONTINFO,
    font: *mut c_void,
    table: u32,
    buffer: *mut u8,
    length: c_ulong,
) -> c_ulong {
    let Some(owner) = (unsafe { default_owner(info) }) else {
        return 0;
    };
    let bytes = if buffer.is_null() {
        &mut []
    } else {
        unsafe { std::slice::from_raw_parts_mut(buffer, length as usize) }
    };
    owner.provider.borrow_mut().as_mut().map_or(0, |provider| {
        provider.get_font_data(font as usize as u64, table, bytes)
    }) as c_ulong
}
unsafe extern "C" fn default_name(
    info: *mut FPDF_SYSFONTINFO,
    font: *mut c_void,
    buffer: *mut c_char,
    length: c_ulong,
) -> c_ulong {
    let Some(owner) = (unsafe { default_owner(info) }) else {
        return 0;
    };
    let Some(name) = owner
        .provider
        .borrow_mut()
        .as_mut()
        .and_then(|provider| provider.get_face_name(font as usize as u64))
    else {
        return 0;
    };
    let needed = name.len() + 1;
    if !buffer.is_null() && needed <= length as usize {
        unsafe {
            std::ptr::copy_nonoverlapping(name.as_ptr(), buffer.cast(), name.len());
            *buffer.add(name.len()) = 0;
        }
    }
    needed as c_ulong
}
unsafe extern "C" fn default_charset(info: *mut FPDF_SYSFONTINFO, font: *mut c_void) -> c_int {
    unsafe { default_owner(info) }
        .and_then(|owner| {
            owner
                .provider
                .borrow_mut()
                .as_mut()
                .and_then(|provider| provider.get_font_charset(font as usize as u64))
        })
        .map_or(0, |charset| charset as c_int)
}
unsafe extern "C" fn default_delete(info: *mut FPDF_SYSFONTINFO, font: *mut c_void) {
    if let Some(owner) = unsafe { default_owner(info) } {
        if let Some(provider) = owner.provider.borrow_mut().as_mut() {
            provider.delete_font(font as usize as u64);
        }
    }
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_GetDefaultSystemFontInfo() -> *mut FPDF_SYSFONTINFO {
    let mut provider = with_ge_module(|module| module.create_default_system_font_info());
    // The default callback table must support MapFont/GetFont without a prior
    // public EnumFonts call, including when wrapped as a version 2 provider.
    let installed_fonts = provider.enum_font_list();
    let owner = Rc::new(DefaultFontInfo {
        interface: FPDF_SYSFONTINFO {
            version: 1,
            Release: Some(default_release),
            EnumFonts: Some(default_enum),
            MapFont: Some(default_map),
            GetFont: Some(default_get),
            GetFontData: Some(default_data),
            GetFaceName: Some(default_name),
            GetFontCharset: Some(default_charset),
            DeleteFont: Some(default_delete),
        },
        provider: RefCell::new(Some(provider)),
        installed_fonts,
    });
    let pointer = Rc::as_ptr(&owner).cast_mut().cast::<FPDF_SYSFONTINFO>();
    DEFAULTS.with(|entries| entries.borrow_mut().insert(pointer as usize, owner));
    pointer
}
/// # Safety
/// `info` is null or was returned by GetDefaultSystemFontInfo and has not been
/// freed. An installed provider retains its real allocation until Release.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_FreeDefaultSystemFontInfo(info: *mut FPDF_SYSFONTINFO) {
    let previous = DEFAULTS.with(|entries| entries.borrow_mut().remove(&(info as usize)));
    drop(previous);
}
#[cfg(test)]
#[path = "sysfontinfo_tests.rs"]
mod tests;
