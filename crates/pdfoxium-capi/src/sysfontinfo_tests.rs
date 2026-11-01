// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use pdfoxium_core::{
    fxcrt::fx_codepage::FxCodePage,
    fxge::{
        cfx_fontmapper::SubstFace,
        cfx_gemodule::{destroy_ge_module, initialize_ge_module},
        cfx_substfont::CfxSubstFont,
    },
};
use std::path::PathBuf;
const HELLO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../upstream/pdfium/testing/resources/hello_world.pdf"
));
fn initialize() {
    destroy_ge_module();
    initialize_ge_module(Some(vec![PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/fonts/ahem"
    ))]));
}
#[repr(C)]
struct Tracking {
    api: FPDF_SYSFONTINFO,
    wrapped: *mut FPDF_SYSFONTINFO,
    enumerations: usize,
    maps: usize,
    releases: usize,
    action: u8,
    replacement: *mut FPDF_SYSFONTINFO,
    nested: bool,
}
impl Tracking {
    fn new(version: c_int) -> Self {
        let wrapped = FPDF_GetDefaultSystemFontInfo();
        Self {
            api: FPDF_SYSFONTINFO {
                version,
                Release: Some(release),
                EnumFonts: Some(enumerate),
                MapFont: Some(map_font),
                GetFont: Some(get_font),
                GetFontData: Some(font_data),
                GetFaceName: Some(face_name),
                GetFontCharset: Some(charset),
                DeleteFont: Some(delete),
            },
            wrapped,
            enumerations: 0,
            maps: 0,
            releases: 0,
            action: 0,
            replacement: std::ptr::null_mut(),
            nested: false,
        }
    }
    fn pointer(&mut self) -> *mut FPDF_SYSFONTINFO {
        std::ptr::addr_of_mut!(self.api)
    }
}
unsafe extern "C" fn release(info: *mut FPDF_SYSFONTINFO) {
    let state = info.cast::<Tracking>();
    let wrapped = unsafe { (*state).wrapped };
    unsafe {
        (*state).releases += 1;
        (*state).wrapped = std::ptr::null_mut();
    }
    if !wrapped.is_null() {
        unsafe {
            (*wrapped).Release.unwrap()(wrapped);
            FPDF_FreeDefaultSystemFontInfo(wrapped);
        }
    }
}
unsafe extern "C" fn enumerate(info: *mut FPDF_SYSFONTINFO, mapper: *mut c_void) {
    let state = info.cast::<Tracking>();
    let wrapped = unsafe { (*state).wrapped };
    unsafe {
        (*state).enumerations += 1;
        (*wrapped).EnumFonts.unwrap()(wrapped, mapper);
    }
}
fn lookup() -> bool {
    lookup_named(b"CallbackFont")
}
fn lookup_named(name: &[u8]) -> bool {
    with_font_manager(|manager| {
        let mut mapper = manager.builtin_mapper_mut();
        matches!(
            mapper.find_subst_face(
                name,
                true,
                0,
                400,
                0,
                FxCodePage::DefAnsi,
                &mut CfxSubstFont::new()
            ),
            Some(SubstFace::External(_))
        )
    })
}
unsafe extern "C" fn map_font(
    info: *mut FPDF_SYSFONTINFO,
    weight: c_int,
    italic: c_int,
    charset: c_int,
    pitch: c_int,
    _face: *const c_char,
    exact: *mut c_int,
) -> *mut c_void {
    let state = info.cast::<Tracking>();
    let (wrapped, action, replacement) =
        unsafe { ((*state).wrapped, (*state).action, (*state).replacement) };
    unsafe {
        (*state).maps += 1;
        (*state).action = 0;
    }
    if action == 1 {
        unsafe {
            FPDF_SetSystemFontInfo(replacement);
        }
        assert_eq!(unsafe { (*state).releases }, 0);
        let result = lookup();
        unsafe {
            (*state).nested = result;
        }
    } else if action == 2 {
        let result = lookup();
        unsafe {
            (*state).nested = result;
        }
    }
    let name = b"Ahem\0";
    unsafe {
        (*wrapped).MapFont.unwrap()(
            wrapped,
            weight,
            italic,
            charset,
            pitch,
            name.as_ptr().cast(),
            exact,
        )
    }
}
unsafe extern "C" fn get_font(info: *mut FPDF_SYSFONTINFO, _face: *const c_char) -> *mut c_void {
    let wrapped = unsafe { (*info.cast::<Tracking>()).wrapped };
    unsafe { (*wrapped).GetFont.unwrap()(wrapped, b"Ahem\0".as_ptr().cast()) }
}
unsafe extern "C" fn font_data(
    info: *mut FPDF_SYSFONTINFO,
    font: *mut c_void,
    table: u32,
    buffer: *mut u8,
    size: c_ulong,
) -> c_ulong {
    let wrapped = unsafe { (*info.cast::<Tracking>()).wrapped };
    unsafe { (*wrapped).GetFontData.unwrap()(wrapped, font, table, buffer, size) }
}
unsafe extern "C" fn face_name(
    info: *mut FPDF_SYSFONTINFO,
    font: *mut c_void,
    buffer: *mut c_char,
    size: c_ulong,
) -> c_ulong {
    let wrapped = unsafe { (*info.cast::<Tracking>()).wrapped };
    unsafe { (*wrapped).GetFaceName.unwrap()(wrapped, font, buffer, size) }
}
unsafe extern "C" fn charset(info: *mut FPDF_SYSFONTINFO, font: *mut c_void) -> c_int {
    let wrapped = unsafe { (*info.cast::<Tracking>()).wrapped };
    unsafe { (*wrapped).GetFontCharset.unwrap()(wrapped, font) }
}
unsafe extern "C" fn delete(info: *mut FPDF_SYSFONTINFO, font: *mut c_void) {
    let wrapped = unsafe { (*info.cast::<Tracking>()).wrapped };
    unsafe {
        (*wrapped).DeleteFont.unwrap()(wrapped, font);
    }
}
unsafe fn render_fixture(bytes: &[u8]) {
    let document =
        unsafe { FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null()) };
    assert!(!document.is_null());
    assert_eq!(unsafe { FPDF_GetPageCount(document) }, 1);
    let page = unsafe { FPDF_LoadPage(document, 0) };
    assert!(!page.is_null());
    let bitmap = FPDFBitmap_Create(200, 200, 1);
    unsafe {
        FPDFBitmap_FillRect(bitmap, 0, 0, 200, 200, 0xffffffff);
        FPDF_RenderPageBitmap(bitmap, page, 0, 0, 200, 200, 0, 0);
    }
    assert_eq!(unsafe { FPDFBitmap_GetWidth(bitmap) }, 200);
    assert_eq!(unsafe { FPDFBitmap_GetHeight(bitmap) }, 200);
    unsafe {
        FPDFBitmap_Destroy(bitmap);
        FPDF_ClosePage(page);
        FPDF_CloseDocument(document);
    }
}
#[test]
fn version1_calls_enum_fonts() {
    initialize();
    let mut info = Tracking::new(1);
    unsafe {
        FPDF_SetSystemFontInfo(info.pointer());
        render_fixture(HELLO);
    }
    assert!(info.enumerations > 0);
    unsafe {
        FPDF_SetSystemFontInfo(std::ptr::null_mut());
    }
    assert_eq!(info.releases, 1);
    destroy_ge_module();
}
#[test]
fn version2_skips_enum_fonts() {
    initialize();
    let mut info = Tracking::new(2);
    unsafe {
        FPDF_SetSystemFontInfo(info.pointer());
        render_fixture(HELLO);
    }
    assert_eq!(info.enumerations, 0);
    assert!(info.maps > 0);
    unsafe {
        FPDF_SetSystemFontInfo(std::ptr::null_mut());
    }
    assert_eq!(info.releases, 1);
    destroy_ge_module();
}
#[test]
fn default_system_font_info() {
    initialize();
    let info = FPDF_GetDefaultSystemFontInfo();
    assert!(!info.is_null());
    unsafe {
        FPDF_SetSystemFontInfo(info);
        render_fixture(HELLO);
        FPDF_SetSystemFontInfo(std::ptr::null_mut());
        FPDF_FreeDefaultSystemFontInfo(info);
    }
    destroy_ge_module();
}
#[test]
fn bug972518() {
    initialize();
    unsafe {
        FPDF_SetSystemFontInfo(std::ptr::null_mut());
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/bug_972518.pdf"
        ));
        let document = FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null());
        assert!(!document.is_null());
        assert_eq!(FPDF_GetPageCount(document), 1);
        let page = FPDF_LoadPage(document, 0);
        assert!(!page.is_null());
        FPDF_ClosePage(page);
        FPDF_CloseDocument(document);
    }
    destroy_ge_module();
}
#[test]
fn default_ttf_map() {
    let mut entry = FPDF_GetDefaultTTFMap();
    assert!(!entry.is_null());
    let mut charsets = Vec::new();
    unsafe {
        while (*entry).charset != -1 && !(*entry).fontname.is_null() {
            charsets.push((*entry).charset);
            entry = entry.add(1);
        }
        assert_eq!((*entry).charset, -1);
        assert!((*entry).fontname.is_null());
    }
    charsets.sort();
    assert_eq!(charsets, vec![0, 128, 129, 134, 136, 178, 204, 238]);
}
#[test]
fn default_ttf_map_count_and_entries() {
    let expected = [
        (0, "Helvetica"),
        (134, "SimSun"),
        (136, "MingLiU"),
        (128, "MS Gothic"),
        (129, "Batang"),
        (204, "Arial"),
        (238, "Arial"),
        (178, "Arial"),
    ];
    assert_eq!(FPDF_GetDefaultTTFMapCount(), expected.len());
    for (index, (charset, name)) in expected.into_iter().enumerate() {
        let entry = FPDF_GetDefaultTTFMapEntry(index);
        assert!(!entry.is_null());
        unsafe {
            assert_eq!((*entry).charset, charset);
            assert_eq!(
                CStr::from_ptr((*entry).fontname).to_bytes(),
                name.as_bytes()
            );
        }
    }
    assert!(FPDF_GetDefaultTTFMapEntry(expected.len()).is_null());
    assert!(FPDF_GetDefaultTTFMapEntry(9999).is_null());
}
#[test]
fn real_default_tables_sizes_names_and_installed_owner_survive_public_free() {
    initialize();
    unsafe {
        let info = FPDF_GetDefaultSystemFontInfo();
        let mut fonts = Enumeration { fonts: Vec::new() };
        (*info).EnumFonts.unwrap()(info, std::ptr::addr_of_mut!(fonts).cast());
        assert!(fonts.fonts.iter().any(|(name, _)| name == b"Ahem"));
        let font = (*info).GetFont.unwrap()(info, b"Ahem\0".as_ptr().cast());
        assert!(!font.is_null());
        let size = (*info).GetFontData.unwrap()(info, font, 0, std::ptr::null_mut(), 0);
        let expected = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
        ));
        assert_eq!(size as usize, expected.len());
        let mut bytes = vec![0; size as usize];
        assert_eq!(
            (*info).GetFontData.unwrap()(info, font, 0, bytes.as_mut_ptr(), size),
            size
        );
        assert_eq!(bytes, expected);
        let name_size = (*info).GetFaceName.unwrap()(info, font, std::ptr::null_mut(), 0);
        let mut name = vec![0x7f; name_size as usize];
        assert_eq!(
            (*info).GetFaceName.unwrap()(info, font, name.as_mut_ptr().cast(), name_size - 1),
            name_size
        );
        assert!(name.iter().all(|&byte| byte == 0x7f));
        assert_eq!(
            (*info).GetFaceName.unwrap()(info, font, name.as_mut_ptr().cast(), name_size),
            name_size
        );
        assert_eq!(name, b"Ahem\0");
        (*info).DeleteFont.unwrap()(info, font);
        FPDF_SetSystemFontInfo(info);
        FPDF_FreeDefaultSystemFontInfo(info);
        assert!(lookup_named(b"Ahem"));
        FPDF_SetSystemFontInfo(std::ptr::null_mut());
    }
    destroy_ge_module();
}
#[test]
fn provider_replacement_and_nested_mapping_release_only_after_outer_request() {
    initialize();
    let mut first = Tracking::new(2);
    let mut second = Tracking::new(2);
    first.action = 1;
    first.replacement = second.pointer();
    unsafe {
        FPDF_SetSystemFontInfo(first.pointer());
    }
    assert!(lookup());
    assert!(first.nested);
    assert_eq!(first.releases, 1);
    assert_eq!(second.releases, 0);
    assert!(lookup());
    unsafe {
        FPDF_SetSystemFontInfo(std::ptr::null_mut());
    }
    assert_eq!(second.releases, 1);
    let mut third = Tracking::new(2);
    third.action = 2;
    unsafe {
        FPDF_SetSystemFontInfo(third.pointer());
    }
    assert!(lookup());
    // Reinstalling one live interface retains its owner rather than releasing
    // its callback data while a newer installation still references it.
    unsafe {
        FPDF_SetSystemFontInfo(third.pointer());
    }
    assert!(lookup());
    assert!(third.nested);
    assert_eq!(third.releases, 0);
    unsafe {
        FPDF_SetSystemFontInfo(std::ptr::null_mut());
    }
    assert_eq!(third.releases, 1);
    destroy_ge_module();
}
