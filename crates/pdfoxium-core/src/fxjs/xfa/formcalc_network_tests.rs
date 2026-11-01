// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::{
    fpdfapi::parser::{
        cpdf_array::CpdfArray, cpdf_document::CpdfDocument, cpdf_object::get_dict_object,
        cpdf_parser::ParserError, cpdf_stream_acc::CpdfRetainedStreamAcc,
    },
    fxcrt::{
        cfx_read_only_container_stream::CfxReadOnlyContainerStream,
        cfx_read_only_span_stream::CfxReadOnlySpanStream, fx_stream::StreamWithSize,
        xml::cfx_xmlparser::CfxXmlParser,
    },
    fxjs::xfa::cfxjse_formcalc_context::CfxJseFormcalcContext,
    xfa::fxfa::{
        fxfa_basic::XfaPacketType,
        parser::{
            cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder,
            cxfa_model_properties::CxfaScriptType,
        },
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::Weak,
};
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
#[derive(Debug, PartialEq)]
enum Request {
    Get(Vec<u32>),
    Post(Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>),
    Put(Vec<u32>, Vec<u32>, Vec<u32>),
}
struct Host {
    runtime: Weak<CfxJseEngine>,
    requests: RefCell<Vec<Request>>,
    missing: Cell<bool>,
    deny: Cell<bool>,
    broken_read: Cell<bool>,
    throw: Cell<bool>,
}
impl FormcalcNetworkHost for Host {
    fn download_url(&self, url: &[u32]) -> JsResult<Option<Rc<dyn SeekableReadStream>>> {
        self.requests.borrow_mut().push(Request::Get(url.to_vec()));
        if self.throw.get() {
            self.runtime
                .upgrade()
                .unwrap()
                .eval("globalThis.nested=pfm_rt.Sum(2,3);throw marker")?;
        }
        if self.missing.get() {
            return Ok(None);
        }
        if self.broken_read.get() {
            return Ok(Some(Rc::new(BrokenRead)));
        }
        Ok(Some(Rc::new(CfxReadOnlyContainerStream::new(
            b"<body>secrets</body>".to_vec(),
        ))))
    }
    fn post_request_url(
        &self,
        url: &[u32],
        data: &[u32],
        content_type: &[u32],
        encoding: &[u32],
        header: &[u32],
    ) -> JsResult<Option<Vec<u32>>> {
        self.requests.borrow_mut().push(Request::Post(
            url.to_vec(),
            data.to_vec(),
            content_type.to_vec(),
            encoding.to_vec(),
            header.to_vec(),
        ));
        Ok((!self.deny.get()).then(|| wide("posted")))
    }
    fn put_request_url(&self, url: &[u32], data: &[u32], encoding: &[u32]) -> JsResult<bool> {
        self.requests.borrow_mut().push(Request::Put(
            url.to_vec(),
            data.to_vec(),
            encoding.to_vec(),
        ));
        Ok(!self.deny.get())
    }
}
struct BrokenRead;
impl StreamWithSize for BrokenRead {
    fn get_size(&self) -> i64 {
        5
    }
}
impl SeekableReadStream for BrokenRead {
    fn read_block_at_offset(&self, _: &mut [u8], _: i64) -> bool {
        false
    }
}
/// Open the native embedder fixture, decode its actual XFA packets, and build
/// the same document model used by these FormCalc methods.
fn fixture() -> (Rc<CfxJseEngine>, Rc<Host>) {
    let mut pdf = CpdfDocument::new();
    assert_eq!(
        pdf.load_doc(
            Rc::new(CfxReadOnlySpanStream::new(include_bytes!(
                "../../../../../upstream/pdfium/testing/resources/simple_xfa.pdf"
            ))),
            b""
        ),
        ParserError::Success
    );
    let root = pdf.root().unwrap();
    let acroform = {
        let borrowed = root.borrow();
        get_dict_object(
            &borrowed
                .as_dictionary()
                .unwrap()
                .get_object_for(b"AcroForm")
                .unwrap(),
        )
        .unwrap()
    };
    let packets = {
        let borrowed = acroform.borrow();
        borrowed
            .as_dictionary()
            .unwrap()
            .get_array_for(b"XFA")
            .unwrap()
    };
    let mut xml = Vec::new();
    {
        let object = packets.borrow();
        let packets = object.as_any().downcast_ref::<CpdfArray>().unwrap();
        for index in (1..packets.size()).step_by(2) {
            let stream = packets.get_direct_object_at(index).unwrap();
            let data = CpdfRetainedStreamAcc::load_filtered(stream).unwrap();
            xml.extend_from_slice(&data.get_span());
        }
    }
    xml.push(0);
    let parsed = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&xml)))
        .parse()
        .unwrap();
    let document = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    assert!(builder.build_document(Rc::new(RefCell::new(parsed)), XfaPacketType::Xdp));
    document.set_root(builder.get_root_node());
    let runtime = CfxJseEngine::new(document.clone()).unwrap();
    document
        .do_data_merge_with_resolver(runtime.resolver())
        .unwrap();
    let host = Rc::new(Host {
        runtime: Rc::downgrade(&runtime),
        requests: RefCell::new(Vec::new()),
        missing: Cell::new(false),
        deny: Cell::new(false),
        broken_read: Cell::new(false),
        throw: Cell::new(false),
    });
    runtime.set_network_host(Some(host.clone()));
    (runtime, host)
}
fn execute(runtime: &CfxJseEngine, source: &str) -> JsValue {
    runtime
        .run_script(CxfaScriptType::Formcalc, &wide(source), None)
        .unwrap()
}
#[test]
fn native_get() {
    let (runtime, host) = fixture();
    assert_eq!(
        execute(&runtime, r#"Get("https://example.com")"#)
            .as_string()
            .unwrap(),
        JsString::from("<body>secrets</body>")
    );
    assert_eq!(
        *host.requests.borrow(),
        vec![Request::Get(wide("https://example.com"))]
    );
}
#[test]
fn native_post() {
    let (runtime, host) = fixture();
    assert_eq!(
        execute(
            &runtime,
            r#"Post("http://example.com", "secret stuff", "text/plain")"#
        )
        .as_string()
        .unwrap(),
        JsString::from("posted")
    );
    assert_eq!(
        *host.requests.borrow(),
        vec![Request::Post(
            wide("http://example.com"),
            wide("secret stuff"),
            wide("text/plain"),
            Vec::new(),
            Vec::new()
        )]
    );
}
#[test]
fn native_put() {
    let (runtime, host) = fixture();
    assert_eq!(
        execute(&runtime, r#"Put("http://example.com", "secret stuff")"#)
            .as_string()
            .unwrap(),
        JsString::default()
    );
    assert_eq!(
        *host.requests.borrow(),
        vec![Request::Put(
            wide("http://example.com"),
            wide("secret stuff"),
            Vec::new()
        )]
    );
}
#[test]
fn defaults_scalar_extraction_coercion_order_and_null_values() {
    let (runtime, host) = fixture();
    assert!(runtime
        .eval("pfm_rt.Post(null,true) === 'posted' && pfm_rt.Put(17,false,null)==='' ")
        .unwrap()
        .to_boolean());
    assert_eq!(
        *host.requests.borrow(),
        vec![
            Request::Post(Vec::new(), wide("1"), Vec::new(), Vec::new(), Vec::new()),
            Request::Put(wide("17"), wide("0"), Vec::new())
        ]
    );
    assert!(runtime.eval("var order=[];function expand(i){return [1,'v',{get v(){order.push('get'+i);return {toString(){order.push('str'+i);return String(i)}}}}]}pfm_rt.Post(expand(1),expand(2),expand(3),expand(4),expand(5))==='posted'&&order.join(',')==='get1,str1,get2,str2,get3,str3,get4,str4,get5,str5'").unwrap().to_boolean());
}
#[test]
fn absent_provider_no_coercion_arity_denied_and_failed_reads() {
    let standalone = CfxJseFormcalcContext::new().unwrap();
    assert!(standalone.engine().eval(&"var used=false;const a=[1,'x',{get x(){used=true;throw 1}}];pfm_rt.Get(a)===undefined&&pfm_rt.Post(a,a)===undefined&&pfm_rt.Put(a,a)===undefined&&!used".encode_utf16().collect::<Vec<_>>()).unwrap().to_boolean());
    for source in [
        "Get()",
        "Get(1,2)",
        "Post()",
        "Post(1)",
        "Post(1,2,3,4,5,6)",
        "Put(1)",
        "Put(1,2,3,4)",
    ] {
        assert!(standalone.execute(&wide(source)).is_err(), "{source}");
    }
    let (runtime, host) = fixture();
    host.missing.set(true);
    assert!(execute(&runtime, r#"Get("missing")"#).is_undefined());
    host.missing.set(false);
    host.broken_read.set(true);
    assert!(runtime.eval("pfm_rt.Get('bad read')").is_err());
    host.deny.set(true);
    for source in ["pfm_rt.Post('url','data')", "pfm_rt.Put('url','data')"] {
        let error = runtime.eval(source).unwrap_err();
        assert!(error
            .to_string()
            .contains("Server does not permit operation"));
    }
}
#[test]
fn opaque_getter_and_reentrant_owner_errors_preserve_js_identity() {
    let (runtime, host) = fixture();
    assert!(runtime.eval("var marker={};var caught=false;try{pfm_rt.Put([1,'x',{get x(){throw marker}}],'data')}catch(e){caught=e===marker}caught").unwrap().to_boolean());
    assert!(host.requests.borrow().is_empty());
    host.throw.set(true);
    assert!(runtime
        .eval("var same=false;try{pfm_rt.Get('url')}catch(e){same=e===marker}same&&nested===5")
        .unwrap()
        .to_boolean());
    host.throw.set(false);
    assert!(runtime
        .eval("pfm_rt.Put('url','data')===''&&pfm_rt.Get('url')==='<body>secrets</body>'")
        .unwrap()
        .to_boolean());
}
