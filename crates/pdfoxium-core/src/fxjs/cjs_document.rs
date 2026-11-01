// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Connected `fxjs/cjs_document.{h,cpp}` metadata, field enumeration/reset,
//! calculation, navigation, annotation/icon bindings and delay queues. Source
//! reader-UI noops and the native word-quad unsupported result are explicit.
use super::cfxjs_engine::CfxJsEngine;
use super::cjs_annot::annot_object;
use super::cjs_delaydata::CjsDelayData;
use super::cjs_event_context::{wide_from_utf16, FieldReference};
use super::cjs_field::{binding_error, field_object};
use super::cjs_icon::{icon_object, is_icon};
use super::cjs_runtime::CjsRuntime;
use super::js_define::expand_keyword_params;
use super::js_resources::JsMessage;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfdoc::cpdf_annot::Subtype;
use crate::fpdfdoc::cpdf_dest::CpdfDest;
use crate::fpdfdoc::cpdf_nametree::CpdfNameTree;
use crate::fpdfsdk::cpdfsdk_formfillenvironment::{ActionError, CpdfSdkFormFillEnvironment};
use crate::fxcrt::fx_string::{utf16_encode, utf8_decode};
use boa_engine::{
    js_string,
    native_function::NativeFunctionPointer,
    object::{builtins::JsArray, FunctionObjectBuilder, JsObject},
    property::PropertyDescriptor,
    Context, JsError, JsResult, JsString, JsValue, NativeFunction,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[path = "cjs_document_words.rs"]
mod words;

#[derive(Default)]
struct DocumentState {
    base_url: RefCell<Vec<u16>>,
    icon_names: RefCell<Vec<Rc<[u16]>>>,
    delay: Cell<bool>,
    delay_data: RefCell<Vec<CjsDelayData>>,
}

// CJS_Document is the runtime's global native object. Native methods require
// that receiver; native properties can be inherited from it as their holder.
fn bound_callback(callback: NativeFunctionPointer, property: bool) -> NativeFunction {
    NativeFunction::from_copy_closure(move |this, args, context| {
        if this.is_null_or_undefined() {
            return callback(this, args, context);
        }
        let global = context.global_object();
        let mut object = this.as_object();
        while let Some(candidate) = object {
            if JsObject::equals(&candidate, &global) {
                return callback(this, args, context);
            }
            if !property {
                break;
            }
            object = candidate.prototype();
        }
        Ok(JsValue::undefined())
    })
}

pub fn register_document(context: &mut Context) -> JsResult<()> {
    context.insert_data(DocumentState::default());
    for (name, callback) in [
        (js_string!("getField"), get_field as NativeFunctionPointer),
        (
            js_string!("getPageNumWords"),
            words::get_page_num_words as NativeFunctionPointer,
        ),
        (
            js_string!("getPageNthWord"),
            words::get_page_nth_word as NativeFunctionPointer,
        ),
        (js_string!("getAnnot"), get_annot as NativeFunctionPointer),
        (js_string!("getAnnots"), get_annots as NativeFunctionPointer),
        (js_string!("addIcon"), add_icon as NativeFunctionPointer),
        (js_string!("getIcon"), get_icon as NativeFunctionPointer),
        (
            js_string!("removeField"),
            remove_field as NativeFunctionPointer,
        ),
        (
            js_string!("getPrintParams"),
            get_print_params as NativeFunctionPointer,
        ),
        (
            js_string!("getPageNthWordQuads"),
            get_page_nth_word_quads as NativeFunctionPointer,
        ),
        (
            js_string!("getNthFieldName"),
            get_nth_field_name as NativeFunctionPointer,
        ),
        (js_string!("resetForm"), reset_form as NativeFunctionPointer),
        (js_string!("print"), print_document as NativeFunctionPointer),
        (
            js_string!("mailDoc"),
            mail_document as NativeFunctionPointer,
        ),
        (js_string!("mailForm"), mail_form as NativeFunctionPointer),
        (
            js_string!("submitForm"),
            submit_form as NativeFunctionPointer,
        ),
        (
            js_string!("calculateNow"),
            calculate_now as NativeFunctionPointer,
        ),
        (
            js_string!("gotoNamedDest"),
            goto_named_dest as NativeFunctionPointer,
        ),
    ] {
        let function = FunctionObjectBuilder::new(context.realm(), bound_callback(callback, false))
            .name(name.clone())
            .length(1)
            .constructor(false)
            .build();
        // DefineMethod/DefineGlobalMethod specify v8::ReadOnly, retaining the
        // default enumerable/configurable flags.
        context.global_object().define_property_or_throw(
            name,
            PropertyDescriptor::builder()
                .value(function)
                .writable(false)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    for name in [
        "addAnnot",
        "addField",
        "addLink",
        "closeDoc",
        "createDataObject",
        "deletePages",
        "exportAsText",
        "exportAsFDF",
        "exportAsXFDF",
        "extractPages",
        "getAnnot3D",
        "getAnnots3D",
        "getLinks",
        "getOCGs",
        "getPageBox",
        "getURL",
        "importAnFDF",
        "importAnXFDF",
        "importTextData",
        "insertPages",
        "replacePages",
        "removeIcon",
        "saveAs",
        "syncAnnotScan",
    ] {
        let function =
            FunctionObjectBuilder::new(context.realm(), bound_callback(source_noop, false))
                .name(JsString::from(name))
                .length(0)
                .constructor(false)
                .build();
        context.global_object().define_property_or_throw(
            JsString::from(name),
            PropertyDescriptor::builder()
                .value(function)
                .writable(false)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    for (name, getter, setter) in [
        (
            "numFields",
            get_num_fields as NativeFunctionPointer,
            set_num_fields as NativeFunctionPointer,
        ),
        ("numPages", get_num_pages, set_num_pages),
        ("pageNum", get_page_num, set_page_num),
        ("dirty", get_dirty, set_dirty),
        ("calculate", get_calculate, set_calculate),
        ("author", get_author, ignore_write),
        ("creationDate", get_creation_date, ignore_write),
        ("creator", get_creator, ignore_write),
        ("keywords", get_keywords, ignore_write),
        ("modDate", get_mod_date, ignore_write),
        ("producer", get_producer, ignore_write),
        ("subject", get_subject, ignore_write),
        ("title", get_title, ignore_write),
        ("info", get_info, set_info),
        ("URL", get_url, set_url),
        ("path", get_path, set_path),
        (
            "documentFileName",
            get_document_file_name,
            set_document_file_name,
        ),
        ("baseURL", get_base_url, set_base_url),
        ("delay", get_delay, set_delay),
        ("icons", get_icons, set_icons),
        ("ADBE", source_noop, source_noop),
        ("bookmarkRoot", source_noop, source_noop),
        ("Collab", source_noop, source_noop),
        ("external", get_external, source_noop),
        ("filesize", get_filesize, set_filesize),
        ("layout", source_noop, source_noop),
        ("media", source_noop, source_noop),
        ("mouseX", source_noop, source_noop),
        ("mouseY", source_noop, source_noop),
        ("pageWindowRect", source_noop, source_noop),
        ("zoom", source_noop, source_noop),
        ("zoomType", source_noop, source_noop),
    ] {
        let getter = FunctionObjectBuilder::new(context.realm(), bound_callback(getter, true))
            .name(JsString::from(format!("get {name}")))
            .length(0)
            .constructor(false)
            .build();
        let setter = FunctionObjectBuilder::new(context.realm(), bound_callback(setter, true))
            .name(JsString::from(format!("set {name}")))
            .length(1)
            .constructor(false)
            .build();
        context.global_object().define_property_or_throw(
            JsString::from(name),
            PropertyDescriptor::builder()
                .get(getter)
                .set(setter)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    Ok(())
}
fn get_field(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let value = args
        .first()
        .ok_or_else(|| binding_error("Document", "getField", JsMessage::ParamError))?;
    let name = value.to_string(context)?.to_vec();
    let environment = CjsRuntime::from_context(context)
        .and_then(|runtime| runtime.environment())
        .ok_or_else(|| binding_error("Document", "getField", JsMessage::BadObjectError))?;
    if environment
        .interactive_form()
        .core_form()
        .count_fields(&wide_from_utf16(&name))
        == 0
    {
        return Ok(JsValue::undefined());
    }
    field_object(
        context,
        FieldReference {
            environment: Rc::downgrade(&environment),
            name: name.into(),
        },
    )
    .map(Into::into)
}
fn goto_named_dest(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(binding_error(
            "Document",
            "gotoNamedDest",
            JsMessage::ParamError,
        ));
    }
    let name = wide_from_utf16(&args[0].to_string(context)?.to_vec());
    let runtime = CjsRuntime::from_context(context)
        .ok_or_else(|| binding_error("Document", "gotoNamedDest", JsMessage::BadObjectError))?;
    let environment = runtime
        .environment()
        .ok_or_else(|| binding_error("Document", "gotoNamedDest", JsMessage::BadObjectError))?;
    let document = environment
        .document()
        .ok_or_else(|| binding_error("Document", "gotoNamedDest", JsMessage::BadObjectError))?;
    let (page, mode, positions) = {
        let mut document = document.borrow_mut();
        let array = CpdfNameTree::lookup_named_dest_unicode(&document, &name)
            .ok_or_else(|| binding_error("Document", "gotoNamedDest", JsMessage::BadObjectError))?;
        let destination = CpdfDest::new(Some(array));
        (
            destination.dest_page_index(&mut document),
            destination.zoom_mode(),
            destination.scroll_position_array(),
        )
    };
    CfxJsEngine::with_host_callback(context, || {
        runtime.with_block(|| {
            let _ = environment.do_go_to_action(page, mode, &positions);
        })
    });
    Ok(JsValue::undefined())
}

fn environment(context: &Context, property: &str) -> JsResult<Rc<CpdfSdkFormFillEnvironment>> {
    CjsRuntime::from_context(context)
        .and_then(|runtime| runtime.environment())
        .ok_or_else(|| binding_error("Document", property, JsMessage::BadObjectError))
}
fn text_value(text: &[u32]) -> JsValue {
    JsString::from(utf16_encode(text).as_slice()).into()
}
fn argument(args: &[JsValue]) -> JsValue {
    args.first().cloned().unwrap_or_else(JsValue::undefined)
}
fn ignore_write(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    // The source metadata setters intentionally accept writes without changing
    // the read-only PDF metadata or coercing the supplied value.
    Ok(JsValue::undefined())
}
macro_rules! readonly {
    ($function:ident, $name:literal) => {
        fn $function(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
            Err(binding_error("Document", $name, JsMessage::ReadOnlyError))
        }
    };
}
readonly!(set_num_fields, "numFields");
readonly!(set_num_pages, "numPages");
readonly!(set_info, "info");
readonly!(set_url, "URL");
readonly!(set_path, "path");
readonly!(set_document_file_name, "documentFileName");
fn get_num_fields(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let environment = environment(context, "numFields")?;
    Ok((environment.interactive_form().core_form().count_fields(&[]) as f64).into())
}
fn get_num_pages(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(environment(context, "numPages")?.get_page_count().into())
}
fn get_nth_field_name(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(binding_error(
            "Document",
            "getNthFieldName",
            JsMessage::ParamError,
        ));
    }
    let index = args[0].to_i32(context)?;
    if index < 0 {
        return Err(binding_error(
            "Document",
            "getNthFieldName",
            JsMessage::ValueError,
        ));
    }
    let field = environment(context, "getNthFieldName")?
        .interactive_form()
        .core_form()
        .get_field(index as usize, &[])
        .ok_or_else(|| binding_error("Document", "getNthFieldName", JsMessage::BadObjectError))?;
    Ok(text_value(&field.full_name()))
}
fn get_dirty(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(environment(context, "dirty")?.change_mark().into())
}
fn set_dirty(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let dirty = argument(args).to_boolean();
    let environment = environment(context, "dirty")?;
    if dirty {
        environment.set_change_mark();
    } else {
        environment.clear_change_mark();
    }
    Ok(JsValue::undefined())
}
fn get_calculate(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(environment(context, "calculate")?
        .interactive_form()
        .is_calculate_enabled()
        .into())
}
fn set_calculate(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let environment = environment(context, "calculate")?;
    environment
        .interactive_form()
        .enable_calculate(argument(args).to_boolean());
    Ok(JsValue::undefined())
}
fn operation_error(
    environment: &CpdfSdkFormFillEnvironment,
    property: &str,
    error: ActionError,
) -> JsError {
    if error == ActionError::JavaScriptException {
        if let Some(error) = environment.last_js_error() {
            return JsError::from_opaque(JsString::from(error.message.as_slice()).into());
        }
    }
    binding_error(
        "Document",
        property,
        if error == ActionError::HostUnavailable {
            JsMessage::NotSupportedError
        } else {
            JsMessage::BadObjectError
        },
    )
}
fn calculate_now(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let environment = environment(context, "calculateNow")?;
    if !environment.has_permissions(0x8 | 0x20 | 0x100) {
        return Err(binding_error(
            "Document",
            "calculateNow",
            JsMessage::PermissionError,
        ));
    }
    let result = CfxJsEngine::with_host_callback(context, || {
        environment.interactive_form().on_calculate(None)
    });
    result.map_err(|error| operation_error(&environment, "calculateNow", error))?;
    if !environment.is_alive() {
        return Err(binding_error(
            "Document",
            "calculateNow",
            JsMessage::BadObjectError,
        ));
    }
    Ok(JsValue::undefined())
}
fn reset_form(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let original = environment(context, "resetForm")?;
    if !original.has_permissions(0x8 | 0x20 | 0x100) {
        return Err(binding_error(
            "Document",
            "resetForm",
            JsMessage::PermissionError,
        ));
    }
    let form = original.interactive_form().core_form();
    if args.is_empty() {
        CfxJsEngine::with_host_callback(context, || form.reset_form());
        if original.is_alive() {
            original.set_change_mark();
        }
        return Ok(JsValue::undefined());
    }
    let mut fields = Vec::new();
    if let Some(name) = args[0].as_string() {
        let name = wide_from_utf16(&name.to_vec());
        for index in 0..form.count_fields(&name) {
            if let Some(field) = form.get_field(index, &name) {
                fields.push(field);
            }
        }
    } else if let Some(array) = args[0].as_object().filter(|object| object.is_array()) {
        let array = JsArray::from_object(array)?;
        let mut index = 0u32;
        while u64::from(index) < array.length(context)? {
            let name = wide_from_utf16(&array.get(index, context)?.to_string(context)?.to_vec());
            // Array getters and string conversions may synchronously detach the
            // document. Retain field identities but reject stale callbacks.
            environment(context, "resetForm")?;
            for field_index in 0..form.count_fields(&name) {
                if let Some(field) = form.get_field(field_index, &name) {
                    fields.push(field);
                }
            }
            index += 1;
        }
    }
    if !fields.is_empty() {
        environment(context, "resetForm")?;
        CfxJsEngine::with_host_callback(context, || form.reset_form_fields(&fields, true));
        if original.is_alive() {
            original.set_change_mark();
        }
    }
    Ok(JsValue::undefined())
}
fn get_page_num(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let environment = environment(context, "pageNum")?;
    let index = CfxJsEngine::with_host_callback(context, || environment.current_page_index());
    if !environment.is_alive() {
        return Err(binding_error(
            "Document",
            "pageNum",
            JsMessage::BadObjectError,
        ));
    }
    Ok(index.map_or_else(JsValue::undefined, JsValue::from))
}
fn set_page_num(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let index = argument(args).to_i32(context)?;
    let environment = environment(context, "pageNum")?;
    let count = environment.get_page_count();
    let index = if index >= count {
        count.saturating_sub(1)
    } else {
        index.max(0)
    };
    CfxJsEngine::with_host_callback(context, || environment.js_doc_goto_page(index))
        .map_err(|error| operation_error(&environment, "pageNum", error))?;
    if !environment.is_alive() {
        return Err(binding_error(
            "Document",
            "pageNum",
            JsMessage::BadObjectError,
        ));
    }
    Ok(JsValue::undefined())
}
fn info_dictionary(context: &Context, property: &str) -> JsResult<ObjectRef> {
    let document = environment(context, property)?
        .document()
        .ok_or_else(|| binding_error("Document", property, JsMessage::BadObjectError))?;
    let info = document
        .borrow_mut()
        .info()
        .ok_or_else(|| binding_error("Document", property, JsMessage::BadObjectError))?;
    Ok(info)
}
fn metadata(context: &Context, property: &str, key: &[u8]) -> JsResult<JsValue> {
    let info = info_dictionary(context, property)?;
    let info = info.borrow();
    let dict = info
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .ok_or_else(|| binding_error("Document", property, JsMessage::BadObjectError))?;
    Ok(text_value(&dict.get_unicode_text_for(key)))
}
macro_rules! metadata_getter {
    ($function:ident, $name:literal, $key:literal) => {
        fn $function(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            metadata(context, $name, $key)
        }
    };
}
metadata_getter!(get_author, "author", b"Author");
metadata_getter!(get_creation_date, "creationDate", b"CreationDate");
metadata_getter!(get_creator, "creator", b"Creator");
metadata_getter!(get_keywords, "keywords", b"Keywords");
metadata_getter!(get_mod_date, "modDate", b"ModDate");
metadata_getter!(get_producer, "producer", b"Producer");
metadata_getter!(get_subject, "subject", b"Subject");
metadata_getter!(get_title, "title", b"Title");
fn get_info(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let info = info_dictionary(context, "info")?;
    let defaults = {
        let borrowed = info.borrow();
        let dict = borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .ok_or_else(|| binding_error("Document", "info", JsMessage::BadObjectError))?;
        [
            b"Author".as_slice(),
            b"Title",
            b"Subject",
            b"Keywords",
            b"Creator",
            b"Producer",
            b"CreationDate",
            b"ModDate",
            b"Trapped",
        ]
        .map(|key| (key, text_value(&dict.get_unicode_text_for(key))))
        .to_vec()
    };
    let result = JsObject::with_object_proto(context.intrinsics());
    for (key, value) in defaults {
        result.set(
            JsString::from(utf16_encode(&utf8_decode(key)).as_slice()),
            value,
            true,
            context,
        )?;
    }
    // Native code snapshots the current info dictionary after the nine default
    // writes. Store only primitive values, ending all PDF borrows before setters
    // on Object.prototype can reenter scripts or mutate the PDF dictionary.
    let extra = {
        let borrowed = info.borrow();
        let dict = borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .ok_or_else(|| binding_error("Document", "info", JsMessage::BadObjectError))?;
        dict.with_locked_entries(|entries| {
            entries
                .iter()
                .filter_map(|(key, object)| {
                    let object = object.borrow();
                    let value = match object.object_type() {
                        ObjectType::String | ObjectType::Name => {
                            text_value(&object.get_unicode_text())
                        }
                        ObjectType::Number => JsValue::from(f64::from(object.get_number())),
                        ObjectType::Boolean => JsValue::from(object.get_integer() != 0),
                        _ => return None,
                    };
                    Some((key.to_vec(), value))
                })
                .collect::<Vec<_>>()
        })
    };
    for (key, value) in extra {
        result.set(
            JsString::from(utf16_encode(&utf8_decode(&key)).as_slice()),
            value,
            true,
            context,
        )?;
    }
    Ok(result.into())
}
fn file_path(context: &mut Context, property: &str) -> JsResult<Vec<u32>> {
    let environment = environment(context, property)?;
    let path = CfxJsEngine::with_host_callback(context, || environment.file_path());
    if !environment.is_alive() {
        return Err(binding_error(
            "Document",
            property,
            JsMessage::BadObjectError,
        ));
    }
    Ok(path)
}
fn get_url(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(text_value(&file_path(context, "URL")?))
}
fn get_path(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    // CJS_App::SysPathToPDFPath, shared here until App is ported.
    let mut result = vec![u32::from(b'/')];
    result.extend(
        file_path(context, "path")?
            .into_iter()
            .filter(|&c| c != u32::from(b':'))
            .map(|c| {
                if c == u32::from(b'\\') {
                    u32::from(b'/')
                } else {
                    c
                }
            }),
    );
    Ok(text_value(&result))
}
fn get_document_file_name(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let path = file_path(context, "documentFileName")?;
    let start = path
        .iter()
        .rposition(|&c| c == u32::from(b'/') || c == u32::from(b'\\'))
        .map(|i| i + 1);
    Ok(text_value(
        start
            .filter(|&i| i < path.len())
            .map_or(&[], |i| &path[i..]),
    ))
}
fn get_base_url(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let value = context
        .get_data::<DocumentState>()
        .expect("registered document")
        .base_url
        .borrow()
        .clone();
    Ok(JsString::from(value.as_slice()).into())
}
fn set_base_url(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let value = argument(args).to_string(context)?.to_vec();
    *context
        .get_data::<DocumentState>()
        .expect("registered document")
        .base_url
        .borrow_mut() = value;
    Ok(JsValue::undefined())
}

fn user_gesture(context: &Context, method: &str) -> JsResult<Rc<CjsRuntime>> {
    let runtime = CjsRuntime::from_context(context)
        .ok_or_else(|| binding_error("Document", method, JsMessage::BadObjectError))?;
    if !runtime
        .current_event_context()
        .is_some_and(|event| event.is_user_gesture())
    {
        return Err(binding_error(
            "Document",
            method,
            JsMessage::UserGestureRequiredError,
        ));
    }
    Ok(runtime)
}
fn print_document(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let params = expand_keyword_params(
        context,
        args,
        &[
            "bUI",
            "nStart",
            "nEnd",
            "bSilent",
            "bShrinkToFit",
            "bPrintAsImage",
            "bReverse",
            "bAnnotations",
        ],
    )?;
    let ui = params[0].as_ref().map_or(true, JsValue::to_boolean);
    let start = params[1]
        .as_ref()
        .map_or(Ok(0), |value| value.to_i32(context))?;
    let end = params[2]
        .as_ref()
        .map_or(Ok(0), |value| value.to_i32(context))?;
    let silent = params[3].as_ref().is_some_and(JsValue::to_boolean);
    let shrink = params[4].as_ref().is_some_and(JsValue::to_boolean);
    let image = params[5].as_ref().is_some_and(JsValue::to_boolean);
    let reverse = params[6].as_ref().is_some_and(JsValue::to_boolean);
    let annotations = params[7].as_ref().is_some_and(JsValue::to_boolean);
    let environment = environment(context, "print")?;
    user_gesture(context, "print")?;
    CfxJsEngine::with_host_callback(context, || {
        environment.js_doc_print(ui, start, end, silent, shrink, image, reverse, annotations)
    });
    if !environment.is_alive() {
        return Err(binding_error(
            "Document",
            "print",
            JsMessage::BadObjectError,
        ));
    }
    Ok(JsValue::undefined())
}
fn mail(context: &mut Context, args: &[JsValue], form_data: bool) -> JsResult<JsValue> {
    let method = if form_data { "mailForm" } else { "mailDoc" };
    let params = expand_keyword_params(
        context,
        args,
        &["bUI", "cTo", "cCc", "cBcc", "cSubject", "cMsg"],
    )?;
    let ui = params[0].as_ref().map_or(true, JsValue::to_boolean);
    let mut text = Vec::with_capacity(5);
    for value in &params[1..] {
        text.push(match value {
            Some(value) => wide_from_utf16(&value.to_string(context)?.to_vec()),
            None => Vec::new(),
        });
    }
    let environment = environment(context, method)?;
    if form_data && !environment.has_permissions(0x200) {
        return Err(binding_error(
            "Document",
            method,
            JsMessage::PermissionError,
        ));
    }
    let runtime = CjsRuntime::from_context(context)
        .ok_or_else(|| binding_error("Document", method, JsMessage::BadObjectError))?;
    // Exporting the FDF queries the host file path and may reenter JavaScript.
    let result = CfxJsEngine::with_host_callback(context, || {
        let data = if form_data {
            environment.interactive_form().export_form_to_fdf_text_buf()
        } else {
            Vec::new()
        };
        if form_data && data.is_empty() {
            return false;
        }
        runtime.with_block(|| {
            environment
                .js_doc_mail_form(&data, ui, &text[0], &text[3], &text[1], &text[2], &text[4])
        });
        true
    });
    if !environment.is_alive() {
        return Err(binding_error("Document", method, JsMessage::BadObjectError));
    }
    if !result {
        return Err(JsError::from_opaque(
            JsString::from("Document.mailForm: Bad FDF format.").into(),
        ));
    }
    Ok(JsValue::undefined())
}
fn mail_document(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    mail(context, args, false)
}
fn mail_form(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    mail(context, args, true)
}
fn submit_form(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.is_empty() {
        return Err(binding_error(
            "Document",
            "submitForm",
            JsMessage::ParamError,
        ));
    }
    let runtime = user_gesture(context, "submitForm")?;
    let mut url = Vec::new();
    let mut fdf = true;
    let mut empty = false;
    let mut array = None;
    if args[0].is_string() {
        url = wide_from_utf16(&args[0].to_string(context)?.to_vec());
        if let Some(value) = args.get(1) {
            fdf = value.to_boolean();
        }
        if let Some(value) = args.get(2) {
            empty = value.to_boolean();
        }
        array = args
            .get(3)
            .and_then(JsValue::as_object)
            .filter(|object| object.is_array());
    } else if let Some(object) = args[0].as_object() {
        // Preserve source property access/coercion order, including getters.
        url = wide_from_utf16(
            &object
                .get(js_string!("cURL"), context)?
                .to_string(context)?
                .to_vec(),
        );
        fdf = object.get(js_string!("bFDF"), context)?.to_boolean();
        empty = object.get(js_string!("bEmpty"), context)?.to_boolean();
        array = object
            .get(js_string!("aFields"), context)?
            .as_object()
            .filter(|object| object.is_array());
    }
    let environment = environment(context, "submitForm")?;
    let sdk = environment.interactive_form();
    let form = sdk.core_form();
    let array = array.map(JsArray::from_object).transpose()?;
    let count = match &array {
        Some(array) => array.length(context)?,
        None => 0,
    };
    if count == 0 && empty {
        if form.check_required_fields(None, true) {
            CfxJsEngine::with_host_callback(context, || {
                runtime.with_block(|| sdk.submit_form(&url))
            })
            .map_err(|error| operation_error(&environment, "submitForm", error))?;
        }
        return Ok(JsValue::undefined());
    }
    let mut fields = Vec::new();
    if let Some(array) = array {
        let mut index = 0u32;
        while u64::from(index) < array.length(context)? {
            let name = wide_from_utf16(&array.get(index, context)?.to_string(context)?.to_vec());
            if !environment.is_alive() {
                return Err(binding_error(
                    "Document",
                    "submitForm",
                    JsMessage::BadObjectError,
                ));
            }
            for field_index in 0..form.count_fields(&name) {
                if let Some(field) = form.get_field(field_index, &name) {
                    if !empty && field.value().is_empty() {
                        continue;
                    }
                    fields.push(field);
                }
            }
            index += 1;
        }
    }
    if form.check_required_fields(Some(&fields), true) {
        CfxJsEngine::with_host_callback(context, || {
            runtime.with_block(|| sdk.submit_fields(&url, &fields, true, !fdf))
        })
        .map_err(|error| operation_error(&environment, "submitForm", error))?;
    }
    Ok(JsValue::undefined())
}

/// Native reader-UI operations explicitly defined as no-ops by the source.
/// These are distinct from missing dependencies (text extraction and XFA).
fn source_noop(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}
fn get_external(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(true.into())
}
fn get_filesize(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(0.into())
}
readonly!(set_filesize, "filesize");
readonly!(set_icons, "icons");
fn get_print_params(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(binding_error(
        "Document",
        "getPrintParams",
        JsMessage::NotSupportedError,
    ))
}
fn get_page_nth_word_quads(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let environment = environment(context, "getPageNthWordQuads")?;
    if !environment.has_permissions(0x200) {
        return Err(binding_error(
            "Document",
            "getPageNthWordQuads",
            JsMessage::BadObjectError,
        ));
    }
    Err(binding_error(
        "Document",
        "getPageNthWordQuads",
        JsMessage::NotSupportedError,
    ))
}
fn get_annot(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 2 {
        return Err(binding_error("Document", "getAnnot", JsMessage::ParamError));
    }
    let page = args[0].to_i32(context)?;
    let name = wide_from_utf16(&args[1].to_string(context)?.to_vec());
    let environment = environment(context, "getAnnot")?;
    let view =
        CfxJsEngine::with_host_callback(context, || environment.get_page_view_at_index(page))
            .filter(|view| view.is_alive())
            .ok_or_else(|| binding_error("Document", "getAnnot", JsMessage::BadObjectError))?;
    let iteration = crate::fpdfsdk::cpdfsdk_annotiteration::CpdfSdkAnnotIteration::new(&view);
    for annotation in iteration.iter() {
        let Some(annotation) = annotation
            .upgrade()
            .filter(|annotation| annotation.is_alive())
        else {
            continue;
        };
        if annotation.as_ba_annot().annot_name() == name {
            return Ok(annot_object(context, &annotation).into());
        }
    }
    Err(binding_error(
        "Document",
        "getAnnot",
        JsMessage::BadObjectError,
    ))
}
fn get_annots(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let environment = environment(context, "getAnnots")?;
    let page_count = environment.get_page_count();
    let result = JsArray::new(context)?;
    // The pinned source does not yet use optional filter arguments.
    for index in 0..page_count {
        let view =
            CfxJsEngine::with_host_callback(context, || environment.get_page_view_at_index(index))
                .filter(|view| view.is_alive())
                .ok_or_else(|| binding_error("Document", "getAnnots", JsMessage::BadObjectError))?;
        let iteration = crate::fpdfsdk::cpdfsdk_annotiteration::CpdfSdkAnnotIteration::new(&view);
        for annotation in iteration.iter() {
            let annotation = annotation
                .upgrade()
                .filter(|annotation| annotation.is_alive())
                .ok_or_else(|| binding_error("Document", "getAnnots", JsMessage::BadObjectError))?;
            if matches!(annotation.subtype(), Subtype::Popup | Subtype::Widget) {
                continue;
            }
            let object = annot_object(context, &annotation);
            result.set(result.length(context)? as u32, object, true, context)?;
        }
    }
    Ok(result.into())
}
fn add_icon(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 2 {
        return Err(binding_error("Document", "addIcon", JsMessage::ParamError));
    }
    if !is_icon(&args[1]) {
        return Err(binding_error("Document", "addIcon", JsMessage::TypeError));
    }
    let name = args[0].to_string(context)?.to_vec();
    context
        .get_data::<DocumentState>()
        .expect("registered document")
        .icon_names
        .borrow_mut()
        .push(name.into());
    Ok(JsValue::undefined())
}
fn get_icons(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if context
        .get_data::<DocumentState>()
        .expect("registered document")
        .icon_names
        .borrow()
        .is_empty()
    {
        return Ok(JsValue::undefined());
    }
    let result = JsArray::new(context)?;
    let mut index = 0u32;
    loop {
        let name = context
            .get_data::<DocumentState>()
            .expect("registered document")
            .icon_names
            .borrow()
            .get(index as usize)
            .cloned();
        let Some(name) = name else { break };
        let icon = icon_object(context, &name);
        // The source list retains stable names across reentrant array setters.
        result.set(index, icon, true, context)?;
        index = index
            .checked_add(1)
            .ok_or_else(|| binding_error("Document", "icons", JsMessage::ValueError))?;
    }
    Ok(result.into())
}
fn get_icon(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(binding_error("Document", "getIcon", JsMessage::ParamError));
    }
    let name = args[0].to_string(context)?.to_vec();
    let name = context
        .get_data::<DocumentState>()
        .expect("registered document")
        .icon_names
        .borrow()
        .iter()
        .find(|candidate| candidate.as_ref() == name.as_slice())
        .cloned()
        .ok_or_else(|| binding_error("Document", "getIcon", JsMessage::BadObjectError))?;
    Ok(icon_object(context, &name).into())
}
fn remove_field(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(binding_error(
            "Document",
            "removeField",
            JsMessage::ParamError,
        ));
    }
    let name = wide_from_utf16(&args[0].to_string(context)?.to_vec());
    let environment = environment(context, "removeField")?;
    if !environment.has_permissions(0x8 | 0x20) {
        return Err(binding_error(
            "Document",
            "removeField",
            JsMessage::PermissionError,
        ));
    }
    let form = environment.interactive_form();
    let widgets = form.widgets_by_name(&name);
    if widgets.is_empty() {
        return Ok(JsValue::undefined());
    }
    // Pinned removeField refreshes existing widgets; it does not erase fields.
    CfxJsEngine::with_host_callback(context, || {
        for widget in widgets {
            if !widget.is_alive() {
                continue;
            }
            let Some(annotation) = widget.annotation() else {
                continue;
            };
            let Some(view) = annotation.page_view() else {
                continue;
            };
            let mut rect = widget.rect();
            rect.inflate(1., 1., 1., 1.);
            view.update_rects(&[rect]);
        }
        if environment.is_alive() {
            environment.set_change_mark();
        }
    });
    Ok(JsValue::undefined())
}
pub fn is_delay(context: &Context) -> bool {
    context
        .get_data::<DocumentState>()
        .is_some_and(|state| state.delay.get())
}
pub fn add_delay_data(context: &Context, data: CjsDelayData) -> JsResult<()> {
    let state = context
        .get_data::<DocumentState>()
        .ok_or_else(|| binding_error("Document", "delay", JsMessage::BadObjectError))?;
    state.delay_data.borrow_mut().push(data);
    Ok(())
}
pub fn do_field_delay(context: &mut Context, name: &[u16], index: i32) -> JsResult<()> {
    let environment = environment(context, "delay")?;
    let selected = {
        let state = context
            .get_data::<DocumentState>()
            .expect("registered document");
        let all = std::mem::take(&mut *state.delay_data.borrow_mut());
        let (selected, remaining): (Vec<_>, Vec<_>) = all
            .into_iter()
            .partition(|data| data.field_name.as_ref() == name && data.control_index == index);
        *state.delay_data.borrow_mut() = remaining;
        selected
    };
    CfxJsEngine::with_host_callback(context, || {
        for data in selected {
            super::cjs_field::do_delay(&environment, data);
        }
    });
    Ok(())
}
fn get_delay(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    environment(context, "delay")?;
    Ok(is_delay(context).into())
}
fn set_delay(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let delay = args.first().is_some_and(JsValue::to_boolean);
    let environment = environment(context, "delay")?;
    if !environment.has_permissions(0x8) {
        return Err(binding_error(
            "Document",
            "delay",
            JsMessage::PermissionError,
        ));
    }
    let queue = {
        let state = context
            .get_data::<DocumentState>()
            .expect("registered document");
        state.delay.set(delay);
        std::mem::take(&mut *state.delay_data.borrow_mut())
    };
    if !delay {
        CfxJsEngine::with_host_callback(context, || {
            for data in queue {
                super::cjs_field::do_delay(&environment, data);
            }
        });
    }
    Ok(JsValue::undefined())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_page::CpdfPage;
    use crate::fpdfapi::parser::{
        cpdf_array::CpdfArray, cpdf_document::CpdfDocument, cpdf_scalar_object::CpdfScalarObject,
        cpdf_string::CpdfString,
    };
    use crate::fpdfdoc::cpdf_aaction::AActionType;
    use crate::fpdfdoc::cpdf_formfield::dict_mut;
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::FormFillHost;
    use crate::fpdfsdk::cpdfsdk_pageview::SdkPage;
    use crate::fpdfsdk::formfiller::cffl_fieldaction::CfflFieldAction;
    use std::rc::Weak;

    #[derive(Default)]
    struct Host {
        environment: RefCell<Weak<CpdfSdkFormFillEnvironment>>,
        current: RefCell<Option<Rc<SdkPage>>>,
        navigated: RefCell<Vec<i32>>,
        destinations: RefCell<Vec<(i32, i32, Vec<f32>)>>,
        printed: RefCell<Vec<(bool, i32, i32, bool, bool, bool, bool, bool)>>,
        mailed: RefCell<Vec<(Vec<u8>, bool, Vec<Vec<u32>>)>>,
        submitted: RefCell<Vec<(Vec<u8>, Vec<u32>)>>,
    }
    impl FormFillHost for Host {
        fn file_path(&self) -> Option<Vec<u32>> {
            Some(wide(r"C:\資料\document.pdf"))
        }
        fn current_page(&self) -> Option<Rc<SdkPage>> {
            self.current.borrow().clone()
        }
        fn do_go_to_action(&self, page: i32, mode: i32, positions: &[f32]) -> bool {
            self.destinations
                .borrow_mut()
                .push((page, mode, positions.to_vec()));
            true
        }
        fn js_doc_goto_page(&self, index: i32) -> bool {
            self.navigated.borrow_mut().push(index);
            let environment = self.environment.borrow().upgrade();
            if let Some(environment) = environment {
                environment
                    .get_or_create_runtime()
                    .unwrap()
                    .run_document_page_script(
                        AActionType::PageOpen,
                        &script("globalThis.pageCallback = (globalThis.pageCallback || 0) + 1;"),
                    )
                    .unwrap();
            }
            true
        }
        fn js_doc_print(
            &self,
            ui: bool,
            start: i32,
            end: i32,
            silent: bool,
            shrink: bool,
            image: bool,
            reverse: bool,
            annotations: bool,
        ) {
            self.printed.borrow_mut().push((
                ui,
                start,
                end,
                silent,
                shrink,
                image,
                reverse,
                annotations,
            ));
        }
        fn js_doc_mail_form(
            &self,
            data: &[u8],
            ui: bool,
            to: &[u32],
            subject: &[u32],
            cc: &[u32],
            bcc: &[u32],
            message: &[u32],
        ) {
            self.mailed.borrow_mut().push((
                data.to_vec(),
                ui,
                [to, subject, cc, bcc, message]
                    .map(<[u32]>::to_vec)
                    .to_vec(),
            ));
        }
        fn submit_form(&self, data: &[u8], destination: &[u32]) -> bool {
            self.submitted
                .borrow_mut()
                .push((data.to_vec(), destination.to_vec()));
            true
        }
    }
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn script(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    fn fixture() -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<Host>,
        Rc<CpdfSdkFormFillEnvironment>,
        Rc<CjsRuntime>,
    ) {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        for index in 0..2 {
            document.borrow_mut().create_new_page(index).unwrap();
        }
        let mut fields = CpdfArray::new();
        for (name, value, default) in [
            ("a", "originalA", "defaultA"),
            ("b", "originalB", "defaultB"),
        ] {
            let mut field = CpdfDictionary::new();
            field.set_new_name_for(b"Subtype", b"Widget");
            field.set_new_name_for(b"FT", b"Tx");
            for (key, text) in [(b"T".as_slice(), name), (b"V", value), (b"DV", default)] {
                field.set_new_for(key, CpdfString::from_unicode(None, &wide(text)));
            }
            fields.append_new(field);
        }
        dict_mut(&document.borrow().root().unwrap(), |root| {
            let mut form = CpdfDictionary::new();
            form.set_new_for(b"Fields", fields);
            root.set_new_for(b"AcroForm", form);
        });
        let info = document.borrow_mut().info().unwrap();
        dict_mut(&info, |info| {
            for (key, text) in [
                (b"Author".as_slice(), "作者"),
                (b"Title", "original title"),
                (b"Subject", "subject"),
                (b"Keywords", "one two"),
                (b"Creator", "creator"),
                (b"Producer", "producer"),
                (b"CreationDate", "D:20261022"),
                (b"ModDate", "D:20261023"),
            ] {
                info.set_new_for(key, CpdfString::from_unicode(None, &wide(text)));
            }
            info.set_new_name_for(b"Trapped", b"False");
            info.set_new_for(b"Rank", CpdfScalarObject::number_from_signed(7));
            info.set_new_for(b"Flag", CpdfScalarObject::boolean(true));
            info.set_new_for(b"Ignored", CpdfArray::new());
        });
        let host = Rc::new(Host::default());
        let interface: Rc<dyn FormFillHost> = host.clone();
        let environment = CpdfSdkFormFillEnvironment::new(&document, Some(&interface));
        *host.environment.borrow_mut() = Rc::downgrade(&environment);
        let runtime = environment.get_or_create_runtime().unwrap();
        (document, host, environment, runtime)
    }
    fn run(runtime: &CjsRuntime, text: &str) {
        runtime.run_document_script(&[], &script(text)).unwrap();
    }
    #[test]
    fn named_destinations_preserve_unicode_tree_keys_and_utf8_legacy_fallback() {
        let (document, host, _environment, runtime) = fixture();
        let destination = |page, top| {
            let mut array = CpdfArray::new();
            array.append_new(CpdfScalarObject::number_from_signed(page));
            array.append_new_name(b"FitH");
            array.append_new(CpdfScalarObject::number_from_signed(top));
            array
        };
        let mut names = CpdfArray::new();
        for name in ["ascii", "café", "Ã©", "Î©", "Ω", "🙂"] {
            names.append_new(CpdfString::from_unicode(None, &wide(name)));
            let target = if name == "Î©" || name == "Ã©" {
                destination(0, 999)
            } else {
                destination(1, 72)
            };
            if name == "Ω" {
                let mut wrapped = CpdfDictionary::new();
                wrapped.set_new_for(b"D", target);
                names.append_new(wrapped);
            } else {
                names.append_new(target);
            }
        }
        dict_mut(&document.borrow().root().unwrap(), |root| {
            let mut modern = CpdfDictionary::new();
            modern.set_new_for(b"Names", names);
            let mut categories = CpdfDictionary::new();
            categories.set_new_for(b"Dests", modern);
            root.set_new_for(b"Names", categories);
            let mut legacy = CpdfDictionary::new();
            legacy.set_new_for("legacy-é".as_bytes(), destination(0, 99));
            root.set_new_for(b"Dests", legacy);
        });
        run(
            &runtime,
            r#"
            let conversions = 0;
            gotoNamedDest({toString() { ++conversions; return 'Ω'; }});
            if (conversions !== 1) throw Error('name conversion count');
            for (const name of ['ascii', 'café', '🙂']) gotoNamedDest(name);
            gotoNamedDest('legacy-é');
            let missing = false;
            try { gotoNamedDest('absent'); } catch (error) { missing = true; }
            if (!missing) throw Error('missing destination must fail');
            gotoNamedDest('Ω');
        "#,
        );
        assert_eq!(
            *host.destinations.borrow(),
            [
                (1, 3, vec![72.]),
                (1, 3, vec![72.]),
                (1, 3, vec![72.]),
                (1, 3, vec![72.]),
                (0, 3, vec![99.]),
                (1, 3, vec![72.]),
            ]
        );
        // A missing Unicode key must not match a different modern name merely
        // because its UTF-8 bytes decode to that name in PDFDocEncoding.
        run(&runtime, "let failed = false; try { gotoNamedDest('é'); } catch (error) { failed = true; } if (!failed) throw Error('wrong alias');");
        assert_eq!(host.destinations.borrow().len(), 6);
    }
    #[test]
    fn real_document_metadata_info_enumeration_and_path_properties() {
        let (_document, _host, environment, runtime) = fixture();
        run(
            &runtime,
            r#"
            function check(value) { if (!value) throw Error('document assertion'); }
            check(author === '作者' && title === 'original title' && subject === 'subject');
            check(keywords === 'one two' && creator === 'creator' && producer === 'producer');
            check(creationDate === 'D:20261022' && modDate === 'D:20261023');
            let details = info;
            check(details.Author === '作者' && details.Title === title && details.Trapped === 'False');
            check(details.Rank === 7 && details.Flag === true && !('Ignored' in details));
            title = {toString() { throw Error('metadata setter must not coerce'); }};
            check(title === 'original title');
            details.Author = 'local'; check(author === '作者');
            check(numFields === 2 && numPages === 2);
            check(getNthFieldName(0) === 'a' && getNthFieldName({valueOf() { return 1; }}) === 'b');
            for (let call of [() => getNthFieldName(), () => getNthFieldName(-1), () => getNthFieldName(2)]) {
                let caught = false; try { call(); } catch (error) { caught = typeof error === 'string'; }
                check(caught);
            }
            for (let property of ['info', 'numFields', 'numPages', 'URL', 'path', 'documentFileName']) {
                let caught = false; try { globalThis[property] = 1; } catch (error) { caught = typeof error === 'string'; }
                check(caught);
            }
            check(URL === 'C:\\資料\\document.pdf' && path === '/C/資料/document.pdf');
            check(documentFileName === 'document.pdf');
            baseURL = {toString() { return 'https://example.test/\ud800'; }};
            check(baseURL.endsWith('\ud800'));
            dirty = true; check(dirty); dirty = false; check(!dirty);
        "#,
        );
        assert!(!environment.change_mark());
    }
    #[test]
    fn info_snapshot_allows_inherited_setters_to_mutate_metadata() {
        let (_document, _host, _environment, runtime) = fixture();
        fn mutate(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            let info = info_dictionary(context, "info")?;
            dict_mut(&info, |info| {
                info.set_new_for(b"Title", CpdfString::from_bytes(None, b"mutated"))
            });
            Ok(JsValue::undefined())
        }
        runtime
            .engine()
            .define_global_method(&script("__mutateInfo"), 0, mutate)
            .unwrap();
        run(
            &runtime,
            r#"
            Object.defineProperty(Object.prototype, 'Author', {configurable: true,
                set(value) { globalThis.lastAuthor = value; __mutateInfo(); }});
            let details = info;
            delete Object.prototype.Author;
            if (lastAuthor !== '作者' || details.Title !== 'mutated' || title !== 'mutated')
                throw Error('info mutation order');
        "#,
        );
    }
    #[test]
    fn reset_calculation_navigation_and_host_services_execute_real_events() {
        let (document, host, environment, runtime) = fixture();
        let page = CpdfPage::from_document(&mut document.borrow_mut(), 1).unwrap();
        *host.current.borrow_mut() = Some(SdkPage::new(&document, &page));
        run(
            &runtime,
            r#"
            function check(value) { if (!value) throw Error('form assertion'); }
            check(pageNum === 1); pageNum = -20; pageNum = 20; pageNum = 1;
            check(pageCallback === 3);
            calculate = false; check(!calculate);
            getField('a').value = 'changedA'; getField('b').value = 'changedB'; dirty = false;
            resetForm('a'); check(getField('a').value === 'defaultA' && getField('b').value === 'changedB' && dirty);
            dirty = false; resetForm(['absent']); check(!dirty);
            resetForm([{toString() { getField('a').value = 'changed again'; return 'b'; }}]);
            check(getField('a').value === 'changed again' && getField('b').value === 'defaultB');
            resetForm(); check(getField('a').value === 'defaultA' && getField('b').value === 'defaultB');
            for (let call of [() => print(), () => submitForm('https://example.test/')]) {
                let caught = false; try { call(); } catch (error) { caught = /User gesture required/.test(error); }
                check(caught);
            }
            mailDoc({bUI: false, cTo:'to', cCc:'cc', cBcc:'bcc', cSubject:'subject', cMsg:'message'});
            mailForm(false, 'form-to', '', '', 'form-subject', 'form-message');
        "#,
        );
        assert_eq!(*host.navigated.borrow(), [0, 1, 1]);
        let form = environment.interactive_form().core_form();
        let a = form.get_field(0, &wide("a")).unwrap();
        let b = form.get_field(0, &wide("b")).unwrap();
        let mut action = CpdfDictionary::new();
        action.set_new_name_for(b"S", b"JavaScript");
        action.set_new_for(
            b"JS",
            CpdfString::from_bytes(None, b"event.value = this.getField('a').value + '!';"),
        );
        dict_mut(&b.field_dict(), |field| {
            let mut actions = CpdfDictionary::new();
            actions.set_new_for(b"C", action);
            field.set_new_for(b"AA", actions);
        });
        dict_mut(&form.form_dict().unwrap(), |dict| {
            let mut order = CpdfArray::new();
            order.append(b.field_dict());
            dict.set_new_for(b"CO", order);
        });
        run(&runtime, "calculateNow(); if (getField('b').value !== 'defaultB') throw Error('disabled calculate'); calculate = true; calculateNow(); if (getField('b').value !== 'defaultA!') throw Error('calculateNow');");
        runtime.run_field_script(&a, AActionType::ButtonUp, &mut CfflFieldAction::default(), &script(r#"
            print({bUI:false, nStart:{valueOf(){return 1;}}, nEnd:2, bSilent:true, bShrinkToFit:true,
                bPrintAsImage:true, bReverse:true, bAnnotations:true});
            submitForm('https://example.test/post', false, false, ['a']);
            submitForm({cURL:'https://example.test/fdf', bFDF:true, bEmpty:true, aFields:[]});
        "#)).unwrap();
        assert_eq!(
            *host.printed.borrow(),
            [(false, 1, 2, true, true, true, true, true)]
        );
        let mailed = host.mailed.borrow();
        assert!(mailed[0].0.is_empty());
        assert!(!mailed[0].1);
        assert_eq!(
            mailed[0].2,
            ["to", "subject", "cc", "bcc", "message"].map(wide)
        );
        assert!(mailed[1].0.starts_with(b"%FDF"));
        let submitted = host.submitted.borrow();
        assert_eq!(
            submitted[0],
            (b"a=defaultA".to_vec(), wide("https://example.test/post"))
        );
        assert!(submitted[1].0.starts_with(b"%FDF"));
        assert_eq!(submitted[1].1, wide("https://example.test/fdf"));
    }
    #[test]
    fn document_and_field_delay_queues_clear_drain_and_match_in_source_order() {
        let (_document, _host, _environment, runtime) = fixture();
        run(
            &runtime,
            r#"
            function check(v){if(!v)throw Error('delay assertion')}
            let a=getField('a'), b=getField('b');
            check(!delay&&!a.delay&&!b.delay);
            a.delay=true;b.delay=true;a.value='discardA';b.value='discardB';
            check(a.value==='originalA'&&b.value==='originalB');
            delay=true;check(delay);
            a.value='A1';b.value='B1';delay=false;
            check(!delay&&a.delay&&b.delay&&a.value==='A1'&&b.value==='B1');
            a.value='A2';b.value='B2';a.value='A3';
            a.delay=false;check(a.value==='A3'&&b.value==='B1'&&!a.delay&&b.delay);
            delay=false;check(b.value==='B2');
            delay=true;a.value='immediate';check(a.value==='immediate');
            b.value='B3';b.delay=false;check(b.value==='B3'&&delay);
        "#,
        );
    }
}

#[cfg(test)]
mod connected_annotation_icon_tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_page::CpdfPage;
    use crate::fpdfapi::parser::{
        cpdf_array::CpdfArray, cpdf_document::CpdfDocument, cpdf_object::retain_object,
        cpdf_scalar_object::CpdfScalarObject, cpdf_string::CpdfString,
    };
    use crate::fpdfdoc::cpdf_formfield::dict_mut;
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::FormFillHost;
    use crate::fpdfsdk::cpdfsdk_pageview::SdkPage;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    use std::rc::Weak;

    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn script(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    #[derive(Default)]
    struct Host {
        environment: RefCell<Weak<CpdfSdkFormFillEnvironment>>,
    }
    impl FormFillHost for Host {
        fn js_app_alert(&self, _: &[u32], _: &[u32], _: i32, _: i32) -> i32 {
            let environment = self.environment.borrow().upgrade();
            if let Some(environment) = environment {
                environment.shutdown();
            }
            1
        }
    }
    fn fixture() -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<Host>,
        Rc<CpdfSdkFormFillEnvironment>,
        Rc<CjsRuntime>,
        Vec<Rc<SdkPage>>,
        ObjectRef,
    ) {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let mut push = CpdfDictionary::new();
        push.set_new_name_for(b"Subtype", b"Widget");
        push.set_new_name_for(b"FT", b"Btn");
        push.set_new_for(b"Ff", CpdfScalarObject::number_from_signed(1 << 16));
        push.set_new_for(b"T", CpdfString::from_bytes(None, b"button"));
        push.set_new_for(b"NM", CpdfString::from_bytes(None, b"widget"));
        push.set_rect_for(b"Rect", CfxFloatRect::new(0., 0., 50., 50.));
        let push = retain_object(push);
        dict_mut(&document.borrow().root().unwrap(), |root| {
            let mut form = CpdfDictionary::new();
            let mut fields = CpdfArray::new();
            fields.append(push.clone());
            form.set_new_for(b"Fields", fields);
            root.set_new_for(b"AcroForm", form);
        });
        let mut square_ref = None;
        for index in 0..2 {
            let dict = document.borrow_mut().create_new_page(index).unwrap();
            let mut entries = CpdfArray::new();
            let mut annotation = CpdfDictionary::new();
            annotation.set_new_name_for(b"Subtype", if index == 0 { b"Square" } else { b"Link" });
            annotation.set_new_for(
                b"NM",
                CpdfString::from_bytes(None, if index == 0 { b"note" } else { b"second" }),
            );
            annotation.set_rect_for(b"Rect", CfxFloatRect::new(10., 10., 30., 30.));
            annotation.set_new_for(b"F", CpdfScalarObject::number_from_signed(4 | 64));
            let annotation = retain_object(annotation);
            if index == 0 {
                square_ref = Some(annotation.clone());
            }
            entries.append(annotation);
            if index == 0 {
                let mut popup = CpdfDictionary::new();
                popup.set_new_name_for(b"Subtype", b"Popup");
                popup.set_new_for(b"NM", CpdfString::from_bytes(None, b"popup"));
                entries.append_new(popup);
                entries.append(push.clone());
            }
            dict_mut(&dict, |dict| dict.set_new_for(b"Annots", entries));
        }
        let host = Rc::new(Host::default());
        let interface: Rc<dyn FormFillHost> = host.clone();
        let environment = CpdfSdkFormFillEnvironment::new(&document, Some(&interface));
        *host.environment.borrow_mut() = Rc::downgrade(&environment);
        let mut pages = Vec::new();
        for index in 0..2 {
            let page = CpdfPage::from_document(&mut document.borrow_mut(), index).unwrap();
            let sdk = SdkPage::new(&document, &page);
            environment.get_or_create_page_view(&sdk).unwrap();
            pages.push(sdk);
        }
        let runtime = environment.get_or_create_runtime().unwrap();
        (
            document,
            host,
            environment,
            runtime,
            pages,
            square_ref.unwrap(),
        )
    }
    #[test]
    fn actual_annotation_lookup_flags_names_order_and_weak_invalidation() {
        let (_document, _host, environment, runtime, pages, square) = fixture();
        runtime.run_document_script(&[],&script(r#"
            function check(v){if(!v)throw Error('annotation assertion')}
            let annotations=getAnnots({ignored:true});
            check(annotations.length===2&&annotations[0].name==='note'&&annotations[1].name==='second');
            let note=getAnnot({valueOf(){return 0}}, {toString(){return 'note'}});
            check(note.type==='Square'&&!note.hidden);
            check(Object.create(note).name==='note'&&Object.create(note).type==='Square');
            check(getAnnot.call({},0,'note')===undefined&&Object.create(globalThis).numPages===2);
            check(getAnnot(0,'widget').type==='Widget');
            note.hidden=true;check(note.hidden);note.hidden=false;check(!note.hidden);
            note.name={toString(){return 'changed\ud800'}};check(note.name==='changed\ud800');
            let threw=false;try{note.type='Link'}catch(e){threw=typeof e==='string'&&e.includes('readonly')}check(threw);
            // CPDF_AnnotList skips serialized Popup dictionaries. Only its
            // generated transient popups become SDK annotation wrappers.
            for(let call of [()=>getAnnot(),()=>getAnnot(9,'absent'),()=>getAnnot(0,'absent'),()=>getAnnot(0,'popup')]){
                threw=false;try{call()}catch(e){threw=typeof e==='string'}check(threw);
            }
            globalThis.retainedNote=note;
        "#)).unwrap();
        let borrowed = square.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        assert_eq!(dict.get_integer_for(b"F", 0), 4 | 64);
        assert_eq!(
            dict.get_unicode_text_for(b"NM"),
            [wide("changed"), vec![0xd800]].concat()
        );
        drop(borrowed);
        let annotation = environment
            .get_page_view(&pages[0])
            .unwrap()
            .annot_by_dict(&square)
            .unwrap();
        annotation.invalidate();
        runtime.run_document_script(&[],&script("for(let p of ['name','type','hidden']){let failed=false;try{retainedNote[p]}catch(e){failed=e.includes('Object no longer exists')}if(!failed)throw Error('stale annotation')}" )).unwrap();
        assert!(Rc::strong_count(&annotation) > 0);
    }
    #[test]
    fn annotation_hidden_rewrites_only_visibility_bits_and_empty_name_removes_key() {
        let (_document, _host, _environment, runtime, _pages, square) = fixture();
        runtime
            .run_document_script(&[], &script("globalThis.note=getAnnot(0,'note')"))
            .unwrap();
        for (value, flags) in [
            ("true", 64 | 1 | 2 | 32),
            ("false", 64 | 4),
            (
                "{valueOf(){throw Error('boolean must not coerce')}}",
                64 | 1 | 2 | 32,
            ),
            ("0", 64 | 4),
        ] {
            runtime
                .run_document_script(&[], &script(&format!("note.hidden={value};")))
                .unwrap();
            assert_eq!(
                square
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .unwrap()
                    .get_integer_for(b"F", 0),
                flags
            );
        }
        runtime
            .run_document_script(
                &[],
                &script("note.name='';if(note.name!=='')throw Error('empty name')"),
            )
            .unwrap();
        assert!(!square
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .key_exist(b"NM"));
    }
    #[test]
    fn name_coercion_rechecks_annotation_after_synchronous_environment_shutdown() {
        let (_document, _host, environment, runtime, _pages, square) = fixture();
        let result=runtime.run_document_script(&[],&script("let note=getAnnot(0,'note');note.name={toString(){app.alert('detach');return 'after detach'}};"));
        assert!(String::from_utf16_lossy(&result.unwrap_err().message)
            .contains("Object no longer exists"));
        assert!(!environment.is_alive());
        assert_eq!(
            square
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_unicode_text_for(b"NM"),
            wide("note")
        );
    }
    #[test]
    fn real_icons_collections_and_source_noops_keep_class_and_memory_semantics() {
        let (_document, _host, _environment, runtime, _pages, _square) = fixture();
        runtime.run_document_script(&[],&script(r#"
            function check(v){if(!v)throw Error('icon assertion')}
            check(icons===undefined);
            let icon=getField('button').buttonGetIcon();check(icon.name===''&&Object.create(icon).name==='');
            check(Object.getOwnPropertyDescriptor(icon,'name').get.call({})===undefined);
            addIcon('one',icon);addIcon({toString(){return 'two\ud800'}},icon);addIcon('one',icon);
            check(icons.length===3&&icons[0].name==='one'&&icons[1].name==='two\ud800'&&icons[2].name==='one');
            check(getIcon('one').name==='one'&&getIcon('one')!==getIcon('one'));
            icons[0].name; removeIcon('one');check(icons.length===3);
            for(let call of [()=>addIcon('fake',{name:''}),()=>addIcon('short'),()=>getIcon('absent'),()=>{icon.name='x'},()=>{icons=[]}]){
                let failed=false;try{call()}catch(e){failed=typeof e==='string'}check(failed);
            }
            let poison={toString(){throw Error('must not coerce')}};
            for(let name of ['addAnnot','addField','addLink','closeDoc','createDataObject','deletePages',
                'exportAsText','exportAsFDF','exportAsXFDF','extractPages','getAnnot3D','getAnnots3D',
                'getLinks','getOCGs','getPageBox','getURL','importAnFDF','importAnXFDF','importTextData',
                'insertPages','replacePages','removeIcon','saveAs','syncAnnotScan'])check(globalThis[name](poison)===undefined);
            for(let name of ['ADBE','bookmarkRoot','Collab','layout','media','mouseX','mouseY','pageWindowRect','zoom','zoomType']){
                check(globalThis[name]===undefined);globalThis[name]=poison;check(globalThis[name]===undefined);
            }
            check(external===true&&filesize===0);external=poison;check(external);
            for(let call of [()=>getPrintParams(),()=>getPageNthWordQuads()]){
                let failed=false;try{call()}catch(e){failed=e.includes('Operation not supported')}check(failed);
            }
        "#)).unwrap();
    }
}

#[cfg(test)]
#[path = "cjs_document_words_tests.rs"]
mod word_tests;
