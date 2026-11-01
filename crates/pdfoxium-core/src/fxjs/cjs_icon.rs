// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_icon.{h,cpp}`. Source Icons contain a name, not image data. Dynamic
//! instances are made by Field.buttonGetIcon and Document's icon collection.
use super::cjs_field::binding_error;
use super::js_resources::JsMessage;
use boa_engine::{
    js_string,
    native_function::NativeFunctionPointer,
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::Attribute,
    Context, JsData, JsObject, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, Trace};

#[derive(Trace, Finalize, JsData)]
struct IconData {
    name: Vec<u16>,
}

// JSPropGetter/Setter inspect the native property holder, including inherited
// accessors. Captures are Copy function pointers and contain no hidden GC roots.
fn property_callback(callback: NativeFunctionPointer) -> NativeFunction {
    NativeFunction::from_copy_closure(move |this, args, context| {
        let mut object = this.as_object();
        while let Some(candidate) = object {
            if candidate.is::<IconData>() {
                return callback(&candidate.into(), args, context);
            }
            object = candidate.prototype();
        }
        Ok(JsValue::undefined())
    })
}

pub fn is_icon(value: &JsValue) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.is::<IconData>())
}
pub fn icon_object(context: &mut Context, name: &[u16]) -> JsObject {
    let getter = FunctionObjectBuilder::new(context.realm(), property_callback(get_name))
        .name(js_string!("get name"))
        .length(0)
        .constructor(false)
        .build();
    let setter = FunctionObjectBuilder::new(context.realm(), property_callback(set_name))
        .name(js_string!("set name"))
        .length(1)
        .constructor(false)
        .build();
    ObjectInitializer::with_native_data(
        IconData {
            name: name.to_vec(),
        },
        context,
    )
    .accessor(
        js_string!("name"),
        Some(getter),
        Some(setter),
        Attribute::all(),
    )
    .build()
}
fn get_name(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let object = this
        .as_object()
        .ok_or_else(|| binding_error("Icon", "name", JsMessage::BadObjectError))?;
    let data = object
        .downcast_ref::<IconData>()
        .ok_or_else(|| binding_error("Icon", "name", JsMessage::BadObjectError))?;
    Ok(JsString::from(data.name.as_slice()).into())
}
fn set_name(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(binding_error("Icon", "name", JsMessage::ReadOnlyError))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_name_uses_real_native_class_and_keeps_utf16() {
        let mut context = Context::default();
        let name = [0xd800, 0, 0xdc00];
        let icon = icon_object(&mut context, &name);
        assert!(is_icon(&icon.clone().into()));
        assert!(!is_icon(
            &JsObject::with_object_proto(context.intrinsics()).into()
        ));
        context
            .register_global_property(js_string!("icon"), icon, Attribute::all())
            .unwrap();
        let actual=context.eval(boa_engine::Source::from_bytes("if(icon.name.length!==3)throw Error('name');let caught=false;try{icon.name={toString(){throw 7}}}catch(e){caught=typeof e==='string'&&e.includes('readonly')}if(!caught)throw Error('setter');icon.name")).unwrap();
        assert_eq!(actual.as_string().unwrap().to_vec(), name);
    }
}
