//! Form queries, script-source inspection and annotation navigation use the
//! same public ABI with native PDFium and the Rust implementation.
mod common;

use pdfium_render::prelude::*;
use std::ffi::{c_int, c_ulong, c_void};
use std::marker::PhantomData;
use std::ptr;

macro_rules! pdf {
    ($file:literal) => {
        include_bytes!(concat!(
            "../../../upstream/pdfium/testing/resources/",
            $file
        ))
    };
}
fn both<T: std::fmt::Debug + PartialEq>(
    test: impl Fn(&dyn PdfiumLibraryBindings, &libloading::Library) -> T + Send + 'static,
) {
    common::with_libraries(move |native, rust| {
        let expected = test(native.as_ref(), native.symbols());
        let actual = test(rust.as_ref(), rust.symbols());
        assert_eq!(actual, expected);
    });
}

struct Document<'a> {
    bindings: &'a dyn PdfiumLibraryBindings,
    handle: FPDF_DOCUMENT,
    _bytes: PhantomData<&'a [u8]>,
}
impl<'a> Document<'a> {
    fn open(bindings: &'a dyn PdfiumLibraryBindings, bytes: &'a [u8]) -> Self {
        let handle = unsafe { bindings.FPDF_LoadMemDocument64(bytes, None) };
        assert!(!handle.is_null());
        Self {
            bindings,
            handle,
            _bytes: PhantomData,
        }
    }
}
impl Drop for Document<'_> {
    fn drop(&mut self) {
        unsafe {
            self.bindings.FPDF_CloseDocument(self.handle);
        }
    }
}
struct Form<'a> {
    document: Document<'a>,
    page: FPDF_PAGE,
    handle: FPDF_FORMHANDLE,
    _info: Box<FPDF_FORMFILLINFO>,
}
impl<'a> Form<'a> {
    fn open(bindings: &'a dyn PdfiumLibraryBindings, bytes: &'a [u8]) -> Self {
        let document = Document::open(bindings, bytes);
        unsafe {
            let page = bindings.FPDF_LoadPage(document.handle, 0);
            assert!(!page.is_null());
            let mut info: Box<FPDF_FORMFILLINFO> = Box::new(std::mem::zeroed());
            info.version = 2;
            info.xfa_disabled = 1;
            let handle = bindings.FPDFDOC_InitFormFillEnvironment(document.handle, &mut *info);
            assert!(!handle.is_null());
            bindings.FORM_OnAfterLoadPage(page, handle);
            Self {
                document,
                page,
                handle,
                _info: info,
            }
        }
    }
}
impl Drop for Form<'_> {
    fn drop(&mut self) {
        unsafe {
            let bindings = self.document.bindings;
            bindings.FORM_OnBeforeClosePage(self.page, self.handle);
            bindings.FPDF_ClosePage(self.page);
            bindings.FPDFDOC_ExitFormFillEnvironment(self.handle);
        }
    }
}

fn text(get: impl Fn(*mut u16, c_ulong) -> c_ulong) -> Option<String> {
    let size = get(ptr::null_mut(), 0);
    if size == 0 {
        return None;
    }
    assert_eq!(size % 2, 0);
    let mut words = vec![0u16; size as usize / 2];
    assert_eq!(get(words.as_mut_ptr(), size), size);
    assert_eq!(words.pop(), Some(0));
    Some(String::from_utf16(&words).unwrap())
}
fn bytes(get: impl Fn(*mut c_void, c_ulong) -> c_ulong) -> Vec<u8> {
    let size = get(ptr::null_mut(), 0);
    let mut bytes = vec![0u8; size as usize];
    assert_eq!(get(bytes.as_mut_ptr().cast(), size), size);
    assert_eq!(bytes.pop(), Some(0));
    bytes
}

#[derive(Debug, PartialEq)]
struct Field {
    kind: c_int,
    name: Option<String>,
    alternate_name: Option<String>,
    value: Option<String>,
    flags: c_int,
    options: Vec<(String, bool)>,
    font_size: Option<f32>,
    font_color: Option<(u32, u32, u32)>,
    checked: bool,
    export: Option<String>,
    controls: c_int,
    control_index: c_int,
}
fn fields(bindings: &dyn PdfiumLibraryBindings, pdf: &[u8]) -> Vec<Field> {
    let form = Form::open(bindings, pdf);
    let mut result = Vec::new();
    unsafe {
        for index in 0..bindings.FPDFPage_GetAnnotCount(form.page) {
            let annot = bindings.FPDFPage_GetAnnot(form.page, index);
            assert!(!annot.is_null());
            let kind = bindings.FPDFAnnot_GetFormFieldType(form.handle, annot);
            if kind < 0 {
                bindings.FPDFPage_CloseAnnot(annot);
                continue;
            }
            let flags = bindings.FPDFAnnot_GetFormFieldFlags(form.handle, annot);
            let options = (0..bindings.FPDFAnnot_GetOptionCount(form.handle, annot).max(0))
                .map(|option| {
                    (
                        text(|buffer, length| {
                            bindings.FPDFAnnot_GetOptionLabel(
                                form.handle,
                                annot,
                                option,
                                buffer,
                                length,
                            )
                        })
                        .unwrap(),
                        bindings.FPDFAnnot_IsOptionSelected(form.handle, annot, option) != 0,
                    )
                })
                .collect();
            let mut size = -1.0;
            let font_size = (bindings.FPDFAnnot_GetFontSize(form.handle, annot, &mut size) != 0)
                .then_some(size);
            let (mut red, mut green, mut blue) = (999, 999, 999);
            let font_color = (bindings.FPDFAnnot_GetFontColor(
                form.handle,
                annot,
                &mut red,
                &mut green,
                &mut blue,
            ) != 0)
                .then_some((red, green, blue));
            let controls = bindings.FPDFAnnot_GetFormControlCount(form.handle, annot);
            let control_index = bindings.FPDFAnnot_GetFormControlIndex(form.handle, annot);
            assert!(controls > 0 && control_index >= 0 && control_index < controls);
            result.push(Field {
                kind,
                name: text(|buffer, length| {
                    bindings.FPDFAnnot_GetFormFieldName(form.handle, annot, buffer, length)
                }),
                alternate_name: text(|buffer, length| {
                    bindings.FPDFAnnot_GetFormFieldAlternateName(form.handle, annot, buffer, length)
                }),
                value: text(|buffer, length| {
                    bindings.FPDFAnnot_GetFormFieldValue(form.handle, annot, buffer, length)
                }),
                flags,
                options,
                font_size,
                font_color,
                checked: bindings.FPDFAnnot_IsChecked(form.handle, annot) != 0,
                export: text(|buffer, length| {
                    bindings.FPDFAnnot_GetFormFieldExportValue(form.handle, annot, buffer, length)
                }),
                controls,
                control_index,
            });
            assert_ne!(
                bindings.FPDFAnnot_SetFormFieldFlags(form.handle, annot, flags ^ 1),
                0
            );
            assert_eq!(
                bindings.FPDFAnnot_GetFormFieldFlags(form.handle, annot),
                flags ^ 1
            );
            assert_ne!(
                bindings.FPDFAnnot_SetFormFieldFlags(form.handle, annot, flags),
                0
            );
            bindings.FPDFPage_CloseAnnot(annot);
        }
    }
    assert!(!result.is_empty());
    result
}

#[test]
fn form_field_queries_and_committed_value_match_native() {
    both(|bindings, library| {
        let all_fields = [
            fields(bindings, pdf!("text_form_color.pdf")),
            fields(bindings, pdf!("combobox_form.pdf")),
            fields(bindings, pdf!("listbox_form.pdf")),
            fields(bindings, pdf!("click_form.pdf")),
        ];
        assert!(all_fields[1]
            .iter()
            .any(|field| field.name.as_deref() == Some("Combo1")
                && field.value.as_deref() == Some("Banana")));
        assert!(all_fields[2].iter().any(|field| field
            .options
            .iter()
            .any(|(label, selected)| label == "Banana" && *selected)));
        assert!(all_fields[3].iter().any(|field| field
            .export
            .as_deref()
            .is_some_and(|value| !value.is_empty())));
        let form = Form::open(bindings, pdf!("text_form.pdf"));
        unsafe {
            let expected: [FPDF_ANNOTATION_SUBTYPE; 2] = [20, 2]; // FPDF_ANNOT_WIDGET, FPDF_ANNOT_LINK.
            assert_ne!(
                bindings.FPDFAnnot_SetFocusableSubtypes(
                    form.handle,
                    expected.as_ptr(),
                    expected.len() as _
                ),
                0
            );
            assert_eq!(bindings.FPDFAnnot_GetFocusableSubtypesCount(form.handle), 2);
            let mut actual = [0; 2];
            assert_ne!(
                bindings.FPDFAnnot_GetFocusableSubtypes(
                    form.handle,
                    actual.as_mut_ptr(),
                    actual.len() as _
                ),
                0
            );
            assert_eq!(actual, expected);
            let annot = bindings.FPDFPage_GetAnnot(form.page, 0);
            assert!(!annot.is_null());
            // These two recent exports are not yet in pdfium-render's trait.
            let get: libloading::Symbol<
                unsafe extern "C" fn(FPDF_FORMHANDLE, FPDF_ANNOTATION) -> c_int,
            > = library.get(b"FORM_GetTextDirection\0").unwrap();
            let set: libloading::Symbol<
                unsafe extern "C" fn(FPDF_FORMHANDLE, FPDF_ANNOTATION, c_int) -> c_int,
            > = library.get(b"FORM_SetTextDirection\0").unwrap();
            assert_eq!(get(form.handle, annot), 1); // Auto.
            for direction in [3, 2, 1] {
                // RTL, LTR, Auto.
                assert_ne!(set(form.handle, annot, direction), 0);
                assert_eq!(get(form.handle, annot), direction);
            }
            assert_ne!(
                bindings.FORM_OnFocus(form.handle, form.page, 0, 115.0, 115.0),
                0
            );
            let replacement: Vec<u16> = "Committed query value"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            bindings.FORM_ReplaceSelection(form.handle, form.page, replacement.as_ptr());
            assert_ne!(bindings.FORM_ForceToKillFocus(form.handle), 0);
            let value = text(|buffer, length| {
                bindings.FPDFAnnot_GetFormFieldValue(form.handle, annot, buffer, length)
            });
            assert_eq!(value.as_deref(), Some("Committed query value"));
            bindings.FPDFPage_CloseAnnot(annot);
            (all_fields, value)
        }
    });
}

fn scripts(bindings: &dyn PdfiumLibraryBindings, pdf: &[u8]) -> Vec<Option<(String, String)>> {
    let doc = Document::open(bindings, pdf);
    unsafe {
        let count = bindings.FPDFDoc_GetJavaScriptActionCount(doc.handle);
        assert!(count > 0);
        (0..count)
            .map(|index| {
                let action = bindings.FPDFDoc_GetJavaScriptAction(doc.handle, index);
                if action.is_null() {
                    return None;
                }
                let name = text(|buffer, length| {
                    bindings.FPDFJavaScriptAction_GetName(action, buffer, length)
                })
                .unwrap();
                let script = text(|buffer, length| {
                    bindings.FPDFJavaScriptAction_GetScript(action, buffer, length)
                })
                .unwrap();
                bindings.FPDFDoc_CloseJavaScriptAction(action);
                Some((name, script))
            })
            .collect()
    }
}
#[test]
fn javascript_action_names_and_sources_match_native() {
    both(|bindings, _| {
        let timer = scripts(bindings, pdf!("bug_679649.pdf"));
        assert_eq!(timer.len(), 1);
        let (name, source) = timer[0].as_ref().unwrap();
        assert_eq!(name, "startDelay");
        assert!(source.contains("app.setTimeOut"));
        let mixed = scripts(bindings, pdf!("js.pdf"));
        assert_eq!(mixed.len(), 5);
        assert!(mixed[..2].iter().all(Option::is_some));
        assert!(mixed[2..].iter().all(Option::is_none));
        (timer, mixed)
    });
}

#[derive(Debug, PartialEq)]
struct Destination {
    page: c_int,
    mode: c_ulong,
    parameters: Vec<f32>,
    location: Option<([bool; 3], [f32; 3])>,
}
unsafe fn destination(
    bindings: &dyn PdfiumLibraryBindings,
    doc: FPDF_DOCUMENT,
    dest: FPDF_DEST,
) -> Option<Destination> {
    if dest.is_null() {
        return None;
    }
    let mut count = 0;
    let mut parameters = [0.0; 4];
    let mode = unsafe { bindings.FPDFDest_GetView(dest, &mut count, parameters.as_mut_ptr()) };
    assert!(count <= 4);
    let (mut has_x, mut has_y, mut has_zoom) = (0, 0, 0);
    let (mut x, mut y, mut zoom) = (-123.0, -123.0, -123.0);
    let located = unsafe {
        bindings.FPDFDest_GetLocationInPage(
            dest,
            &mut has_x,
            &mut has_y,
            &mut has_zoom,
            &mut x,
            &mut y,
            &mut zoom,
        )
    };
    Some(Destination {
        page: unsafe { bindings.FPDFDest_GetDestPageIndex(doc, dest) },
        mode,
        parameters: parameters[..count as usize].to_vec(),
        location: (located != 0).then_some(([has_x != 0, has_y != 0, has_zoom != 0], [x, y, zoom])),
    })
}
#[derive(Debug, PartialEq)]
struct Bookmark {
    title: String,
    depth: usize,
    count: c_int,
    action_kind: c_ulong,
    destination: Option<Destination>,
}
unsafe fn bookmarks(
    bindings: &dyn PdfiumLibraryBindings,
    doc: FPDF_DOCUMENT,
    parent: FPDF_BOOKMARK,
    depth: usize,
    result: &mut Vec<Bookmark>,
) {
    assert!(depth < 8 && result.len() < 32);
    let mut item = unsafe { bindings.FPDFBookmark_GetFirstChild(doc, parent) };
    while !item.is_null() {
        assert!(result.len() < 32);
        let title = text(|buffer, length| unsafe {
            bindings.FPDFBookmark_GetTitle(item, buffer.cast(), length)
        })
        .unwrap();
        result.push(Bookmark {
            title,
            depth,
            count: unsafe { bindings.FPDFBookmark_GetCount(item) },
            action_kind: unsafe {
                bindings.FPDFAction_GetType(bindings.FPDFBookmark_GetAction(item))
            },
            destination: unsafe {
                destination(bindings, doc, bindings.FPDFBookmark_GetDest(doc, item))
            },
        });
        unsafe {
            bookmarks(bindings, doc, item, depth + 1, result);
        }
        item = unsafe { bindings.FPDFBookmark_GetNextSibling(doc, item) };
    }
}

#[test]
fn link_actions_bookmarks_destinations_labels_and_metadata_match_native() {
    both(|bindings, _| unsafe {
        let uri = Document::open(bindings, pdf!("uri_action.pdf"));
        let page = bindings.FPDF_LoadPage(uri.handle, 0);
        assert!(!page.is_null());
        let link = bindings.FPDFLink_GetLinkAtPoint(page, 100.0, 100.0);
        assert!(!link.is_null());
        assert_eq!(
            bindings.FPDFLink_GetLinkZOrderAtPoint(page, 100.0, 100.0),
            0
        );
        let action = bindings.FPDFLink_GetAction(link);
        assert!(!action.is_null());
        let kind = bindings.FPDFAction_GetType(action);
        assert_eq!(kind, 3); // PDFACTION_URI.
        let path = bytes(|buffer, length| {
            bindings.FPDFAction_GetURIPath(uri.handle, action, buffer, length)
        });
        assert_eq!(path, b"https://example.com/page.html");
        let mut rect: FS_RECTF = std::mem::zeroed();
        assert_ne!(bindings.FPDFLink_GetAnnotRect(link, &mut rect), 0);
        assert!(rect.left < rect.right && rect.bottom < rect.top);
        let annot = bindings.FPDFLink_GetAnnot(page, link);
        assert!(!annot.is_null());
        assert_eq!(bindings.FPDFAnnot_GetLink(annot), link);
        bindings.FPDFPage_CloseAnnot(annot);
        let mut position = 0;
        let mut enumerated = ptr::null_mut();
        assert_ne!(
            bindings.FPDFLink_Enumerate(page, &mut position, &mut enumerated),
            0
        );
        assert_eq!(enumerated, link);
        bindings.FPDF_ClosePage(page);

        let book = Document::open(bindings, pdf!("bookmarks.pdf"));
        let mut outline = Vec::new();
        bookmarks(bindings, book.handle, ptr::null_mut(), 0, &mut outline);
        assert_eq!(outline[0].title, "A Good Beginning");
        let descendant = outline
            .iter()
            .find(|entry| entry.title == "Open Middle Descendant")
            .unwrap();
        let dest = descendant.destination.as_ref().unwrap();
        assert_eq!(dest.page, 0);
        assert_eq!(
            dest.location,
            Some(([true, true, false], [100.0, 200.0, -123.0]))
        );
        let found = bindings.FPDFBookmark_Find_str(book.handle, "open middle descendant");
        assert!(!found.is_null());
        assert_eq!(
            text(|buffer, length| bindings.FPDFBookmark_GetTitle(found, buffer.cast(), length))
                .as_deref(),
            Some("Open Middle Descendant")
        );
        let labels = Document::open(bindings, pdf!("page_labels.pdf"));
        let page_labels: Vec<_> = (0..bindings.FPDF_GetPageCount(labels.handle))
            .map(|index| {
                text(|buffer, length| {
                    bindings.FPDF_GetPageLabel(labels.handle, index, buffer.cast(), length)
                })
                .unwrap()
            })
            .collect();
        assert_eq!(page_labels, ["i", "ii", "1", "2", "zzA", "zzB", ""]);
        let metadata = Document::open(bindings, pdf!("bug_601362.pdf"));
        let creator = text(|buffer, length| {
            bindings.FPDF_GetMetaText(metadata.handle, "Creator", buffer.cast(), length)
        });
        assert_eq!(creator.as_deref(), Some("Microsoft Word"));
        (
            kind,
            path,
            [rect.left, rect.bottom, rect.right, rect.top],
            outline,
            page_labels,
            creator,
        )
    });
}
