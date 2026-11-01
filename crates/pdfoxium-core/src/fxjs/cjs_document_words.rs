// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Word extraction callbacks from `fxjs/cjs_document.cpp`. This source API
//! traverses active top-level text objects in content order, unlike TextPage's
//! reading-order extraction. Native getPageNthWordQuads remains unsupported.
use super::*;
use crate::fpdfapi::page::{cpdf_page::CpdfPage, cpdf_textobject::CpdfTextObject};
use crate::fpdfapi::parser::{cpdf_array::CpdfArray, cpdf_object::get_direct_object};
use std::collections::HashSet;
fn raw(dict: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    dict.borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)
}
fn resolve(mut object: ObjectRef) -> Option<ObjectRef> {
    let mut seen = HashSet::new();
    loop {
        if object.borrow().object_type() != ObjectType::Reference {
            return Some(object);
        }
        if !seen.insert(Rc::as_ptr(&object) as *const () as usize) {
            return None;
        }
        object = get_direct_object(&object)?;
    }
}
fn direct(dict: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    resolve(raw(dict, key)?)
}
fn page_dictionary(root: &ObjectRef, index: usize) -> Option<ObjectRef> {
    let pages = direct(root, b"Pages")?;
    // CPDF_Document's /Count skip and /Kids traversal, with owned frames so
    // input callbacks may query the document without an outer document borrow.
    let mut stack = vec![(pages, 0usize)];
    let mut remaining = index;
    let mut seen = HashSet::new();
    while let Some((object, depth)) = stack.pop() {
        let Some(node) = resolve(object) else {
            continue;
        };
        if depth >= 1024 || !seen.insert(Rc::as_ptr(&node) as *const () as usize) {
            continue;
        }
        if node.borrow().object_type() != ObjectType::Dictionary {
            continue;
        }
        let kids = direct(&node, b"Kids");
        if kids.is_none() {
            if remaining == 0 {
                return Some(node);
            }
            remaining -= 1;
            continue;
        }
        let count = direct(&node, b"Count").map_or(0, |o| o.borrow().get_integer());
        if count > 0 && (count as usize) <= remaining {
            remaining -= count as usize;
            continue;
        }
        let kids = kids?;
        let size = {
            let array = kids.borrow();
            array.as_any().downcast_ref::<CpdfArray>()?.size()
        };
        for child in (0..size).rev() {
            let object = kids
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()?
                .get_object_at(child);
            if let Some(object) = object {
                stack.push((object, depth + 1));
            }
        }
    }
    None
}
fn load_page(environment: &CpdfSdkFormFillEnvironment, index: i32) -> Result<CpdfPage, JsMessage> {
    let document = environment.document().ok_or(JsMessage::BadObjectError)?;
    let (root, page_data, count) = {
        let document = document.borrow();
        (
            document.root(),
            document.page_data_ref(),
            document.page_count(),
        )
    };
    if index < 0 || index as usize >= count {
        return Err(JsMessage::ValueError);
    }
    let dict = page_dictionary(&root.ok_or(JsMessage::BadObjectError)?, index as usize)
        .ok_or(JsMessage::BadObjectError)?;
    let mut page =
        CpdfPage::new_with_page_data(dict, page_data).ok_or(JsMessage::BadObjectError)?;
    page.parse_path_content();
    if !environment.is_alive() {
        return Err(JsMessage::BadObjectError);
    }
    Ok(page)
}
pub(super) fn get_page_num_words(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let index = match args.first() {
        Some(value) => value.to_i32(context)?,
        None => 0,
    };
    let environment = environment(context, "getPageNumWords")?;
    if !environment.has_permissions(0x200) {
        return Err(binding_error(
            "Document",
            "getPageNumWords",
            JsMessage::PermissionError,
        ));
    }
    let page = CfxJsEngine::with_host_callback(context, || load_page(&environment, index))
        .map_err(|message| binding_error("Document", "getPageNumWords", message))?;
    let count = page
        .holder()
        .iter()
        .filter(|object| object.base().is_active())
        .filter_map(|object| object.as_any().downcast_ref::<CpdfTextObject>())
        .map(CpdfTextObject::count_words)
        .fold(0usize, usize::saturating_add);
    Ok((count as f64).into())
}
pub(super) fn get_page_nth_word(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let index = match args.first() {
        Some(value) => value.to_i32(context)?,
        None => 0,
    };
    let mut word = match args.get(1) {
        Some(value) => value.to_i32(context)?,
        None => 0,
    };
    let strip = args.get(2).is_none_or(JsValue::to_boolean);
    let environment = environment(context, "getPageNthWord")?;
    if !environment.has_permissions(0x200) {
        return Err(binding_error(
            "Document",
            "getPageNthWord",
            JsMessage::PermissionError,
        ));
    }
    let page = CfxJsEngine::with_host_callback(context, || load_page(&environment, index))
        .map_err(|message| binding_error("Document", "getPageNthWord", message))?;
    let mut result = Vec::new();
    if word >= 0 {
        for object in page
            .holder()
            .iter()
            .filter(|object| object.base().is_active())
        {
            let Some(text) = object.as_any().downcast_ref::<CpdfTextObject>() else {
                continue;
            };
            let count = text.count_words();
            // Native >= incorrectly selects the preceding object at boundaries.
            if (word as usize) < count {
                result = text.get_word_string(word);
                break;
            }
            word = word.saturating_sub(i32::try_from(count).unwrap_or(i32::MAX));
        }
    }
    if strip {
        return Ok(text_value(crate::fxcrt::widestring::trim_whitespace(
            &result,
        )));
    }

    Ok(text_value(&result))
}
