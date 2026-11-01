// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_console.{h,cpp}`. All four methods are intentional source no-ops;
//! PDFium does not forward console output to the embedding application's UI.
use boa_engine::{
    js_string,
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::Attribute,
    Context, JsResult, JsValue, NativeFunction,
};
pub fn register_console(context: &mut Context) -> JsResult<()> {
    let methods: Vec<_> = ["clear", "hide", "println", "show"]
        .into_iter()
        .map(|name| {
            let function =
                FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(noop))
                    .name(name)
                    .length(0)
                    .constructor(false)
                    .build();
            (name, function)
        })
        .collect();
    let mut object = ObjectInitializer::new(context);
    for (name, function) in methods {
        object.property(
            js_string!(name),
            function,
            Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
        );
    }
    let object = object.build();
    context.register_global_property(js_string!("console"), object, Attribute::all())
}
fn noop(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_source_console_methods_are_noops_without_argument_coercion() {
        let mut context = Context::default();
        register_console(&mut context).unwrap();
        assert_eq!(context.eval(boa_engine::Source::from_bytes("let poison={toString(){throw 7;}}; [console.clear(poison),console.hide(poison),console.println(poison),console.show(poison)].every(x=>x===undefined);")).unwrap().as_boolean(),Some(true));
        assert_eq!(context.eval(boa_engine::Source::from_bytes("let d=Object.getOwnPropertyDescriptor(console,'println'); !d.writable && d.enumerable && d.configurable && !('prototype' in console.println);")).unwrap().as_boolean(),Some(true));
    }
}
