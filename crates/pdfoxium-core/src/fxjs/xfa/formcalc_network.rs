// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Source Get/Post/Put through actual embedding services. This module opens no
//! sockets and installs no HTTP client. Opaque owner/coercion errors propagate
//! through the existing synchronous Boa context lease.
use super::{
    cfxjse_engine::CfxJseEngine,
    formcalc_values::{exception, get_simple_value, param_count, value_to_utf8_string},
};
use crate::{
    fxcrt::{
        fx_stream::SeekableReadStream,
        fx_string::{utf16_encode, utf8_decode},
    },
    fxjs::cfxjs_engine::CfxJsEngine,
};
use boa_engine::{native_function::NativeFunctionPointer, Context, JsResult, JsString, JsValue};
use std::rc::Rc;
pub trait FormcalcNetworkHost {
    fn download_url(&self, url: &[u32]) -> JsResult<Option<Rc<dyn SeekableReadStream>>>;
    fn post_request_url(
        &self,
        url: &[u32],
        data: &[u32],
        content_type: &[u32],
        encoding: &[u32],
        header: &[u32],
    ) -> JsResult<Option<Vec<u32>>>;
    fn put_request_url(&self, url: &[u32], data: &[u32], encoding: &[u32]) -> JsResult<bool>;
}
pub(super) const METHODS: &[(&str, NativeFunctionPointer)] =
    &[("Get", get), ("Post", post), ("Put", put)];
fn argument(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<Vec<u32>> {
    let value = get_simple_value(args, index, context)?;
    Ok(utf8_decode(&value_to_utf8_string(&value, context)?))
}
fn string(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}
fn denied() -> boa_engine::JsError {
    exception("Server does not permit operation.")
}
fn get(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(param_count("Get"));
    }
    let Some(host) = CfxJseEngine::from_context(context).and_then(|runtime| runtime.network_host())
    else {
        return Ok(JsValue::undefined());
    };
    let url = argument(args, 0, context)?;
    let Some(file) = CfxJsEngine::with_host_callback(context, || host.download_url(&url))? else {
        return Ok(JsValue::undefined());
    };
    let bytes = CfxJsEngine::with_host_callback(context, || {
        let size = usize::try_from(file.get_size()).map_err(|_| denied())?;
        if size > i32::MAX as usize {
            return Err(denied());
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size).map_err(|_| denied())?;
        bytes.resize(size, 0);
        // Native ignores ReadBlockAtOffset failure and returns initialized NUL
        // data. A failed owner read is an actual failed operation, not content.
        if size != 0 && !file.read_block_at_offset(&mut bytes, 0) {
            return Err(denied());
        }
        Ok(bytes)
    })?;
    Ok(string(&utf8_decode(&bytes)))
}
fn post(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if !(2..=5).contains(&args.len()) {
        return Err(param_count("Post"));
    }
    let Some(host) = CfxJseEngine::from_context(context).and_then(|runtime| runtime.network_host())
    else {
        return Ok(JsValue::undefined());
    };
    let url = argument(args, 0, context)?;
    let data = argument(args, 1, context)?;
    let content_type = if args.len() > 2 {
        argument(args, 2, context)?
    } else {
        Vec::new()
    };
    let encoding = if args.len() > 3 {
        argument(args, 3, context)?
    } else {
        Vec::new()
    };
    let header = if args.len() > 4 {
        argument(args, 4, context)?
    } else {
        Vec::new()
    };
    let response = CfxJsEngine::with_host_callback(context, || {
        host.post_request_url(&url, &data, &content_type, &encoding, &header)
    })?
    .ok_or_else(denied)?;
    Ok(string(&response))
}
fn put(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if !(2..=3).contains(&args.len()) {
        return Err(param_count("Put"));
    }
    let Some(host) = CfxJseEngine::from_context(context).and_then(|runtime| runtime.network_host())
    else {
        return Ok(JsValue::undefined());
    };
    let url = argument(args, 0, context)?;
    let data = argument(args, 1, context)?;
    let encoding = if args.len() > 2 {
        argument(args, 2, context)?
    } else {
        Vec::new()
    };
    if !CfxJsEngine::with_host_callback(context, || host.put_request_url(&url, &data, &encoding))? {
        return Err(denied());
    }
    Ok(JsString::default().into())
}
#[cfg(test)]
#[path = "formcalc_network_tests.rs"]
mod tests;
