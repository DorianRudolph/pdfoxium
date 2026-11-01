// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Portable context/class adapters from cfxjse_context/class and the native
//! ArrayBuffer allocation policy. Boa remains the sole VM/allocator/date host.
use boa_engine::{
    context::{DefaultHooks, HostHooks},
    Context, JsResult,
};
#[cfg(any(feature = "xfa", test))]
use boa_engine::{
    object::FunctionObjectBuilder, property::PropertyDescriptor, JsObject, JsString, JsSymbol,
    NativeFunction, Source,
};
use std::rc::Rc;
pub(crate) const MAX_ARRAY_BUFFER_BYTES: u64 = 0x1000_0000;
#[derive(Debug, Default)]
pub(crate) struct PdfHostHooks;
impl HostHooks for PdfHostHooks {
    fn max_buffer_size(&self, _: &mut Context) -> u64 {
        MAX_ARRAY_BUFFER_BYTES
    }
    fn local_timezone_offset_seconds(&self, seconds: i64) -> i32 {
        // Use exactly the selected Boa host implementation, including its
        // platform timezone support. No UTC override or alternate clock.
        DefaultHooks.local_timezone_offset_seconds(seconds)
    }
}
/// Applies to library-owned VMs. Injected contexts and external SDK VMs retain
/// their configured host hooks and actual clock, as native external isolates do.
pub(crate) fn new_context() -> JsResult<Context> {
    Context::builder().host_hooks(Rc::new(PdfHostHooks)).build()
}
#[cfg(any(feature = "xfa", test))]
const COMPATIBLE_MODE_SCRIPT:&[u8]=br#"(function(global,list){
'use strict';
for(const objname in list){
 const globalobj=global[objname];
 if(globalobj){for(const name of list[objname]){
  if(!globalobj[name])Object.defineProperty(globalobj,name,{
   writable:true,enumerable:false,value:function(obj){
    if(arguments.length===0)throw new TypeError('missing argument 0 when calling function '+objname+'.'+name);
    return globalobj.prototype[name].apply(obj,Array.prototype.slice.call(arguments,1));
   }
  });
 }}
}
})(this,{String:['substr','toUpperCase']});"#;
#[cfg(any(feature = "xfa", test))]
const CONSOLE_SCRIPT: &[u8] = br#"console.show=function(){};
console.println=function(...args){this.log(...args);};"#;
#[cfg(any(feature = "xfa", test))]
pub(crate) fn enable_compatible_mode(context: &mut Context) -> JsResult<()> {
    context.eval(Source::from_bytes(COMPATIBLE_MODE_SCRIPT))?;
    // Boa has no built-in console. Use the existing actual CJS_Console
    // implementation (native intentional no-ops), then provide the log alias
    // needed by the source compatibility script. No output host is invented.
    let global = context.global_object();
    let mut bootstrapped = false;
    let console = if let Some(object) = global
        .get(boa_engine::js_string!("console"), context)?
        .as_object()
    {
        object
    } else {
        super::cjs_console::register_console(context)?;
        bootstrapped = true;
        global
            .get(boa_engine::js_string!("console"), context)?
            .as_object()
            .expect("registered console")
    };
    if bootstrapped {
        for name in ["show", "println"] {
            let name = JsString::from(name);
            let value = console.get(name.clone(), context)?;
            console.define_property_or_throw(
                name,
                PropertyDescriptor::builder()
                    .value(value)
                    .writable(true)
                    .enumerable(true)
                    .configurable(true),
                context,
            )?;
        }
    }
    if console
        .get(boa_engine::js_string!("log"), context)?
        .is_undefined()
    {
        let log = console.get(boa_engine::js_string!("println"), context)?;
        console.define_property_or_throw(
            boa_engine::js_string!("log"),
            PropertyDescriptor::builder()
                .value(log)
                .writable(true)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    context.eval(Source::from_bytes(CONSOLE_SCRIPT))?;
    Ok(())
}
#[cfg(any(feature = "xfa", test))]
pub(crate) fn set_class_tag(object: &JsObject, name: &str, context: &mut Context) -> JsResult<()> {
    object.define_property_or_throw(
        JsSymbol::to_string_tag(),
        PropertyDescriptor::builder()
            .value(JsString::from(name))
            .writable(false)
            .enumerable(false)
            .configurable(true),
        context,
    )?;
    Ok(())
}
#[cfg(any(feature = "xfa", test))]
pub(crate) fn set_global_class(context: &mut Context, name: &str) -> JsResult<()> {
    let global = context.global_object();
    set_class_tag(&global, "global", context)?;
    let display = JsString::from(format!("[object {name}]"));
    let function = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |_, _, display, _| Ok(display.clone().into()),
            display,
        ),
    )
    .name("toString")
    .constructor(false)
    .build();
    global.define_property_or_throw(
        boa_engine::js_string!("toString"),
        PropertyDescriptor::builder()
            .value(function)
            .writable(true)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    Ok(())
}
/// Install after subclass overrides so nondeletable base methods do not prevent
/// the native Packet/Model overrides during construction.
#[cfg(feature = "xfa")]
pub(crate) fn finish_class(object: &JsObject, context: &mut Context) -> JsResult<()> {
    set_class_tag(object, "XFAObject", context)?;
    for key in object.own_property_keys(context)? {
        if matches!(key, boa_engine::property::PropertyKey::Symbol(_)) {
            continue;
        }
        object.define_property_or_throw(
            key,
            PropertyDescriptor::builder().configurable(false),
            context,
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "boa_adapters_tests.rs"]
mod tests;
