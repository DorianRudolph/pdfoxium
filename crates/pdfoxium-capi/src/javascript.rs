// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! JavaScript action inspection from `fpdfsdk/fpdf_javascript.cpp`.
//! These owned name/source snapshots do not execute scripts or retain a
//! document. Runtime execution belongs to the SDK JavaScript boundary.
use super::*;
use pdfoxium_core::fpdfdoc::cpdf_action::{ActionType, CpdfAction};
use pdfoxium_core::fpdfdoc::cpdf_nametree::CpdfNameTree;
use pdfoxium_core::fxcrt::fx_string::utf16_encode;

struct JavaScriptAction {
    name: Vec<u32>,
    script: Vec<u32>,
}

unsafe fn name_tree(document: *mut c_void) -> Option<CpdfNameTree> {
    // Release the document borrow before lazy name-tree reads may enter the
    // embedder's file callbacks. No raw handle reference crosses those reads.
    let root = {
        let handle = unsafe { document.cast::<DocumentHandle>().as_ref() }?;
        let root = handle.document.borrow().root();
        root?
    };
    CpdfNameTree::create_from_root(&root, b"JavaScript")
}

/// # Safety
/// `document` is a live document handle from this library, or null.
#[no_mangle]
pub unsafe extern "C" fn FPDFDoc_GetJavaScriptActionCount(document: *mut c_void) -> c_int {
    if document.is_null() {
        return -1;
    }
    unsafe { name_tree(document) }.map_or(0, |tree| {
        c_int::try_from(tree.count()).unwrap_or(c_int::MAX)
    })
}

/// # Safety
/// `document` is a live document handle from this library, or null. The caller
/// owns the returned action and closes it with FPDFDoc_CloseJavaScriptAction.
#[no_mangle]
pub unsafe extern "C" fn FPDFDoc_GetJavaScriptAction(
    document: *mut c_void,
    index: c_int,
) -> *mut c_void {
    let Ok(index) = usize::try_from(index) else {
        return std::ptr::null_mut();
    };
    let Some(tree) = (unsafe { name_tree(document) }) else {
        return std::ptr::null_mut();
    };
    if index >= tree.count() {
        return std::ptr::null_mut();
    }
    let Some((object, name)) = tree.lookup_value_and_name(index) else {
        return std::ptr::null_mut();
    };
    // Type is optional, but must be /Action if present. An empty string or
    // stream is a valid script; a missing /JS or another object type is not.
    let action = CpdfAction::new(Some(object));
    if action.action_type() != ActionType::JavaScript {
        return std::ptr::null_mut();
    }
    let Some(script) = action.maybe_javascript() else {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(JavaScriptAction { name, script })).cast()
}

/// # Safety
/// `javascript` is a caller-owned live action returned by
/// FPDFDoc_GetJavaScriptAction, or null. This call consumes the handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFDoc_CloseJavaScriptAction(javascript: *mut c_void) {
    if !javascript.is_null() {
        drop(unsafe { Box::from_raw(javascript.cast::<JavaScriptAction>()) });
    }
}

unsafe fn copy_text(text: &[u32], buffer: *mut u16, buflen: c_ulong) -> c_ulong {
    let bytes: Vec<u8> = utf16_encode(text)
        .into_iter()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    if !buffer.is_null() && buflen as usize >= bytes.len() {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), bytes.len());
        }
    }
    bytes.len() as c_ulong
}

/// # Safety
/// `javascript` is live or null. If non-null, `buffer` points to at least
/// `buflen` writable bytes. Returns the UTF-16LE byte count including the NUL.
#[no_mangle]
pub unsafe extern "C" fn FPDFJavaScriptAction_GetName(
    javascript: *mut c_void,
    buffer: *mut u16,
    buflen: c_ulong,
) -> c_ulong {
    let Some(action) = (unsafe { javascript.cast::<JavaScriptAction>().as_ref() }) else {
        return 0;
    };
    unsafe { copy_text(&action.name, buffer, buflen) }
}

/// # Safety
/// `javascript` is live or null. If non-null, `buffer` points to at least
/// `buflen` writable bytes. Returns the UTF-16LE byte count including the NUL.
#[no_mangle]
pub unsafe extern "C" fn FPDFJavaScriptAction_GetScript(
    javascript: *mut c_void,
    buffer: *mut u16,
    buflen: c_ulong,
) -> c_ulong {
    let Some(action) = (unsafe { javascript.cast::<JavaScriptAction>().as_ref() }) else {
        return 0;
    };
    unsafe { copy_text(&action.script, buffer, buflen) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdfoxium_core::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
    use pdfoxium_core::fpdfapi::parser::cpdf_reference::CpdfReference;
    use pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use pdfoxium_core::fpdfapi::parser::cpdf_stream::CpdfStream;
    use pdfoxium_core::fpdfapi::parser::cpdf_string::CpdfString;

    const BUG: &[u8] = include_bytes!("../../../upstream/pdfium/testing/resources/bug_679649.pdf");
    const JS: &[u8] = include_bytes!("../../../upstream/pdfium/testing/resources/js.pdf");
    const HELLO: &[u8] =
        include_bytes!("../../../upstream/pdfium/testing/resources/hello_world.pdf");

    fn load(bytes: &[u8]) -> *mut c_void {
        let doc =
            unsafe { FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null()) };
        assert!(!doc.is_null());
        doc
    }
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    unsafe fn get_text(
        action: *mut c_void,
        getter: unsafe extern "C" fn(*mut c_void, *mut u16, c_ulong) -> c_ulong,
    ) -> String {
        let size = unsafe { getter(action, std::ptr::null_mut(), 0) };
        assert!(size >= 2 && size % 2 == 0);
        let mut buffer = vec![0u16; size as usize / 2];
        assert_eq!(unsafe { getter(action, buffer.as_mut_ptr(), size) }, size);
        assert_eq!(buffer.pop(), Some(0));
        String::from_utf16(&buffer).unwrap()
    }
    unsafe fn check_buffer_contract(
        action: *mut c_void,
        getter: unsafe extern "C" fn(*mut c_void, *mut u16, c_ulong) -> c_ulong,
        expected: &str,
        size: c_ulong,
    ) {
        let mut null_buffer = [0xabcd; 10];
        assert_eq!(
            unsafe { getter(std::ptr::null_mut(), std::ptr::null_mut(), 0) },
            0
        );
        assert_eq!(
            unsafe { getter(std::ptr::null_mut(), null_buffer.as_mut_ptr(), 0) },
            0
        );
        assert_eq!(
            unsafe { getter(std::ptr::null_mut(), null_buffer.as_mut_ptr(), 20) },
            0
        );
        assert_eq!(null_buffer, [0xabcd; 10]);
        assert_eq!(unsafe { getter(action, std::ptr::null_mut(), 0) }, size);
        let mut buffer = vec![0xccccu16; size as usize / 2];
        buffer[..4].copy_from_slice(&[0x6261, 0x6463, 0x6665, 0x6867]);
        let original = buffer.clone();
        assert_eq!(
            unsafe { getter(action, buffer.as_mut_ptr(), size - 1) },
            size
        );
        assert_eq!(buffer, original);
        assert_eq!(unsafe { getter(action, buffer.as_mut_ptr(), size) }, size);
        assert_eq!(buffer.pop(), Some(0));
        assert_eq!(String::from_utf16(&buffer).unwrap(), expected);
    }

    // FPDFJavaScriptEmbedderTest.CountJS
    #[test]
    fn count_js() {
        let doc = load(BUG);
        unsafe {
            assert_eq!(FPDFDoc_GetJavaScriptActionCount(doc), 1);
            FPDF_CloseDocument(doc);
        }
    }
    // FPDFJavaScriptEmbedderTest.CountNoJS
    #[test]
    fn count_no_js() {
        let doc = load(HELLO);
        unsafe {
            assert_eq!(FPDFDoc_GetJavaScriptActionCount(doc), 0);
            assert_eq!(FPDFDoc_GetJavaScriptActionCount(std::ptr::null_mut()), -1);
            FPDF_CloseDocument(doc);
        }
    }
    // FPDFJavaScriptEmbedderTest.GetJS
    #[test]
    fn get_js() {
        let doc = load(JS);
        unsafe {
            assert_eq!(FPDFDoc_GetJavaScriptActionCount(doc), 5);
            for index in [-1, 5] {
                assert!(FPDFDoc_GetJavaScriptAction(doc, index).is_null());
            }
            for index in [-1, 0, 1, 2, 5] {
                assert!(FPDFDoc_GetJavaScriptAction(std::ptr::null_mut(), index).is_null());
            }
            for index in 0..5 {
                let action = FPDFDoc_GetJavaScriptAction(doc, index);
                assert_eq!(!action.is_null(), index < 2);
                FPDFDoc_CloseJavaScriptAction(action);
            }
            FPDF_CloseDocument(doc);
        }
    }
    // FPDFJavaScriptEmbedderTest.GetJSName
    #[test]
    fn get_js_name() {
        let doc = load(BUG);
        unsafe {
            let action = FPDFDoc_GetJavaScriptAction(doc, 0);
            assert!(!action.is_null());
            check_buffer_contract(action, FPDFJavaScriptAction_GetName, "startDelay", 22);
            FPDFDoc_CloseJavaScriptAction(action);
            FPDF_CloseDocument(doc);
        }
    }
    // FPDFJavaScriptEmbedderTest.GetJSScript
    #[test]
    fn get_js_script() {
        let doc = load(BUG);
        unsafe {
            let action = FPDFDoc_GetJavaScriptAction(doc, 0);
            assert!(!action.is_null());
            check_buffer_contract(
                action,
                FPDFJavaScriptAction_GetScript,
                "function ping() {\n  app.alert(\"ping\");\n}\nvar timer = app.setTimeOut(\"ping()\", 100);\napp.clearTimeOut(timer);",
                218,
            );
            FPDFDoc_CloseJavaScriptAction(action);
            FPDF_CloseDocument(doc);
        }
    }

    fn add_action(tree: &CpdfNameTree, name: &str, kind: Option<&[u8]>, script: Option<ObjectRef>) {
        let mut dict = CpdfDictionary::new();
        if let Some(kind) = kind {
            dict.set_new_name_for(b"Type", kind);
        }
        dict.set_new_name_for(b"S", b"JavaScript");
        dict.set_for(b"JS", script);
        assert!(tree.add_value_and_name(retain_object(dict), &wide(name)));
    }
    #[test]
    fn optional_type_empty_unicode_stream_and_owned_lifetime() {
        let doc = FPDF_CreateNewDocument();
        let (document, tree) = {
            let handle = unsafe { &*doc.cast::<DocumentHandle>() };
            let document = handle.document.clone();
            let tree = CpdfNameTree::create_with_root_name_array(&document.borrow(), b"JavaScript")
                .unwrap();
            (document, tree)
        };
        add_action(
            &tree,
            "A absent Type",
            None,
            Some(retain_object(CpdfString::from_unicode(
                None,
                &wide("'𝄞漢字';"),
            ))),
        );
        add_action(
            &tree,
            "B empty",
            Some(b"Action"),
            Some(retain_object(CpdfString::from_bytes(None, b""))),
        );
        let stream_reference = {
            let objects = document.borrow().holder();
            let number = objects
                .borrow_mut()
                .add_indirect_object(retain_object(CpdfStream::from_bytes(b"streamScript();")));
            retain_object(CpdfReference::new(&objects, number))
        };
        add_action(&tree, "C stream", Some(b"Action"), Some(stream_reference));
        add_action(
            &tree,
            "D malformed Type",
            Some(b"Wrong"),
            Some(retain_object(CpdfString::from_bytes(None, b"ignored();"))),
        );
        add_action(&tree, "E missing JS", None, None);
        add_action(
            &tree,
            "F wrong JS type",
            None,
            Some(retain_object(CpdfScalarObject::number_from_signed(7))),
        );
        assert!(tree.add_value_and_name(
            retain_object(CpdfScalarObject::number_from_signed(8)),
            &wide("G nondictionary")
        ));
        let expected = ["'𝄞漢字';", "", "streamScript();"];
        unsafe {
            assert_eq!(FPDFDoc_GetJavaScriptActionCount(doc), 7);
            let actions: Vec<_> = (0..3)
                .map(|index| FPDFDoc_GetJavaScriptAction(doc, index))
                .collect();
            assert!(actions.iter().all(|action| !action.is_null()));
            for index in 3..7 {
                assert!(FPDFDoc_GetJavaScriptAction(doc, index).is_null());
            }
            // Snapshot handles own their strings, not document/tree pointers.
            drop(tree);
            drop(document);
            FPDF_CloseDocument(doc);
            for (index, action) in actions.into_iter().enumerate() {
                assert_eq!(
                    get_text(action, FPDFJavaScriptAction_GetScript),
                    expected[index]
                );
                assert!(!get_text(action, FPDFJavaScriptAction_GetName).is_empty());
                FPDFDoc_CloseJavaScriptAction(action);
            }
        }
    }
}
