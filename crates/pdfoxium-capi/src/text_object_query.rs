// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! fpdf_edittext.cpp::FPDFTextObj_GetText, using the actual extraction identity.
use super::*;
#[no_mangle]
pub unsafe extern "C" fn FPDFTextObj_GetText(
    text_object: *mut c_void,
    text_page: *mut c_void,
    buffer: *mut u16,
    length: c_ulong,
) -> c_ulong {
    if !unsafe {
        with_page_object_ref(text_object, |object| {
            object.object_type()
                == pdfoxium_core::fpdfapi::page::cpdf_pageobject::PageObjectType::Text
        })
    }
    .unwrap_or(false)
    {
        return 0;
    }
    let Some(page) = (unsafe { text_page.cast::<TextPageHandle>().as_ref() }) else {
        return 0;
    };
    let id = match unsafe { &(*text_object.cast::<PageObjectHandle>()).location } {
        ObjectLocation::Attached { id, .. }
        | ObjectLocation::InForm { id, .. }
        | ObjectLocation::InAnnotation { id, .. } => Some(*id),
        ObjectLocation::Owned(_) => None,
        ObjectLocation::Transferring => return 0,
    };
    let text = id.map_or_else(Vec::new, |id| page.text.get_text_by_object_id(id));
    unsafe { doc::copy_utf16(&text, buffer.cast(), length) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extraction_uses_object_identity_and_byte_sized_buffers() {
        unsafe {
            let document = FPDF_CreateNewDocument();
            let page = FPDFPage_New(document, 0, 400., 400.);
            let object = FPDFPageObj_NewTextObj(document, c"Helvetica".as_ptr(), 12.);
            let input: Vec<u16> = "Hello world".encode_utf16().chain(Some(0)).collect();
            assert_eq!(FPDFText_SetText(object, input.as_ptr()), 1);
            FPDFPageObj_Transform(object, 1., 0., 0., 1., 20., 300.);
            FPDFPage_InsertObject(page, object);
            assert_eq!(FPDFPage_GenerateContent(page), 1);
            let text = FPDFText_LoadPage(page);
            let length = (input.len() * 2) as c_ulong;
            assert_eq!(
                FPDFTextObj_GetText(object, text, std::ptr::null_mut(), 0),
                length
            );
            let mut output = vec![0xa5a5; input.len() + 1];
            assert_eq!(
                FPDFTextObj_GetText(object, text, output.as_mut_ptr(), length - 1),
                length
            );
            assert_eq!(output, vec![0xa5a5; input.len() + 1]);
            assert_eq!(
                FPDFTextObj_GetText(object, text, output.as_mut_ptr(), length),
                length
            );
            assert_eq!(&output[..input.len()], input);
            assert_eq!(output[input.len()], 0xa5a5);
            let foreign = FPDFPageObj_NewTextObj(document, c"Helvetica".as_ptr(), 12.);
            let mut terminator = 0xa5a5;
            assert_eq!(FPDFTextObj_GetText(foreign, text, &mut terminator, 2), 2);
            assert_eq!(terminator, 0);
            let path = FPDFPageObj_CreateNewRect(0., 0., 10., 10.);
            assert_eq!(FPDFTextObj_GetText(path, text, &mut terminator, 2), 0);
            assert_eq!(
                FPDFTextObj_GetText(object, std::ptr::null_mut(), &mut terminator, 2),
                0
            );
            assert_eq!(
                FPDFTextObj_GetText(std::ptr::null_mut(), text, &mut terminator, 2),
                0
            );
            FPDFPageObj_Destroy(path);
            FPDFPageObj_Destroy(foreign);
            FPDFText_ClosePage(text);
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
        }
    }
}
