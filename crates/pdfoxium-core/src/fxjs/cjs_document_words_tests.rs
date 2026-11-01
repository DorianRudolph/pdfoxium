// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fpdfapi::parser::{
    cpdf_document::CpdfDocument, cpdf_indirect_object_holder::HolderRef,
    cpdf_object::retain_object, cpdf_reference::CpdfReference, cpdf_stream::CpdfStream,
};
use crate::fpdfdoc::cpdf_formfield::dict_mut;
fn script(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}
fn indirect(holder: &HolderRef, object: ObjectRef) -> ObjectRef {
    let number = holder.borrow_mut().add_indirect_object(object);
    retain_object(CpdfReference::new(holder, number))
}
fn fixture() -> (
    Rc<RefCell<CpdfDocument>>,
    Rc<CpdfSdkFormFillEnvironment>,
    Rc<CjsRuntime>,
) {
    let document = Rc::new(RefCell::new(CpdfDocument::new()));
    document.borrow_mut().create_new_doc();
    let holder = document.borrow().holder();
    let mut font = CpdfDictionary::new();
    font.set_new_name_for(b"Type", b"Font");
    font.set_new_name_for(b"Subtype", b"Type1");
    font.set_new_name_for(b"BaseFont", b"Helvetica");
    font.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
    let normal = indirect(&holder, retain_object(font));
    let mut mapped = CpdfDictionary::new();
    mapped.set_new_name_for(b"Type", b"Font");
    mapped.set_new_name_for(b"Subtype", b"Type1");
    mapped.set_new_name_for(b"BaseFont", b"Helvetica");
    mapped.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
    mapped.set_for(b"ToUnicode",Some(indirect(&holder,retain_object(CpdfStream::from_bytes(b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapType 2 def 1 begincodespacerange <00> <ff> endcodespacerange 3 beginbfchar <41> <4e00> <42> <4e8c> <43> <d83dde00> endbfchar endcmap end end")))));
    let mapped = indirect(&holder, retain_object(mapped));
    let mut fonts = CpdfDictionary::new();
    fonts.set_for(b"F1", Some(normal));
    fonts.set_for(b"F2", Some(mapped));
    let mut resources = CpdfDictionary::new();
    resources.set_for(b"Font", Some(retain_object(fonts)));
    let resources = retain_object(resources);
    let nested = CpdfStream::from_bytes(b"BT /F1 12 Tf 0 0 Td (Nested ignored) Tj ET");
    let nested_dict = nested.get_dict();
    dict_mut(&nested_dict, |dict| {
        dict.set_new_name_for(b"Type", b"XObject");
        dict.set_new_name_for(b"Subtype", b"Form");
        dict.set_rect_for(
            b"BBox",
            crate::fxcrt::fx_coordinates::CfxFloatRect::new(0.0, 0.0, 100.0, 100.0),
        );
        dict.set_for(b"Resources", Some(resources.borrow().clone_object()));
    });
    let nested = indirect(&holder, retain_object(nested));
    let mut xobjects = CpdfDictionary::new();
    xobjects.set_for(b"Fm", Some(nested));
    dict_mut(&resources, |dict| {
        dict.set_for(b"XObject", Some(retain_object(xobjects)))
    });
    for(index,content)in [b"BT /F1 12 Tf 1 0 0 1 20 100 Tm (  Hello, world!  ) Tj ET BT /F1 12 Tf 1 0 0 1 20 200 Tm (Second third) Tj ET /Fm Do".as_slice(),b"BT /F2 12 Tf 20 100 Td (ABC) Tj ET"].into_iter().enumerate(){
        let page=document.borrow_mut().create_new_page(index as i32).unwrap();let contents=indirect(&holder,retain_object(CpdfStream::from_bytes(content)));dict_mut(&page,|dict|{dict.set_for(b"Resources",Some(resources.borrow().clone_object()));dict.set_for(b"Contents",Some(contents));});
    }
    let environment = CpdfSdkFormFillEnvironment::new(&document, None);
    let runtime = environment.get_or_create_runtime().unwrap();
    (document, environment, runtime)
}
#[test]
fn actual_document_word_apis_preserve_object_order_boundaries_cjk_and_spaces() {
    let (_document, _environment, runtime) = fixture();
    runtime.run_document_script(&[],&script(r#"
        let wordAssertion=0; function check(value){wordAssertion++;if(!value)throw Error('word assertion '+wordAssertion);}
        check(getPageNumWords()===4 && getPageNumWords(0, 'ignored')===4);
        check(getPageNthWord()==='Hello,' && getPageNthWord(0,0,false)==='Hello, ');
        check(getPageNthWord(0,1)==='world!' && getPageNthWord(0,1,false)==='world!  ');
        check(getPageNthWord(0,2)==='Second' && getPageNthWord(0,3)==='third');
        check(getPageNthWord(0,4)==='' && getPageNthWord(0,-1,false)==='');
        check(getPageNumWords(1)===3 && getPageNthWord(1,0)==='一' && getPageNthWord(1,1)==='二' && getPageNthWord(1,2)==='😀');
        let order='';check(getPageNthWord({valueOf(){order+='p';check(numPages===2);return 0}}, {valueOf(){order+='w';return 2}}, {valueOf(){throw Error('Boolean must not coerce')}})==='Second');check(order==='pw');
        let marker={};for(let call of [()=>getPageNumWords({valueOf(){throw marker}}),()=>getPageNthWord(0,{valueOf(){throw marker}})]){let caught;try{call()}catch(error){caught=error}check(caught===marker);}
        for(let call of [()=>getPageNumWords(-1),()=>getPageNthWord(2,0)]){let caught;try{call()}catch(error){caught=error}check(typeof caught==='string');}
        let coerced=false;try{getPageNthWordQuads({valueOf(){coerced=true;return 0}})}catch(error){check(error.includes('Operation not supported'))}check(!coerced);
        check(getPageNumWords.call({})===undefined && getPageNthWord.call({},0,0)===undefined);
    "#)).unwrap();
}
#[test]
fn word_argument_coercion_can_detach_actual_environment() {
    let (_document, _environment, runtime) = fixture();
    runtime
        .engine()
        .with_context(|context| {
            context.register_global_builtin_callable(
                js_string!("detachDuringCoercion"),
                0,
                NativeFunction::from_fn_ptr(|_, _, context| {
                    let runtime = CjsRuntime::from_context(context).unwrap();
                    if let Some(environment) = runtime.environment() {
                        CfxJsEngine::with_host_callback(context, || environment.shutdown());
                    }
                    Ok(0.into())
                }),
            )?;
            Ok(())
        })
        .unwrap();
    let result=runtime.run_document_script(&[],&script("globalThis.wordDetachedRejected=false;try{getPageNumWords({valueOf(){detachDuringCoercion();return 0}})}catch(error){wordDetachedRejected=typeof error==='string'}if(!wordDetachedRejected)throw Error('detached word request succeeded');"));
    result.unwrap();
    assert!(runtime
        .engine()
        .eval(&script("globalThis.wordDetachedRejected === true"))
        .unwrap()
        .to_boolean());
    assert!(runtime.environment().is_none());
}

#[test]
fn lazy_word_content_reads_allow_same_context_and_document_query_reentry() {
    use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};
    struct Input {
        bytes: Vec<u8>,
        callback: RefCell<Option<Rc<dyn Fn()>>>,
        in_callback: Cell<bool>,
    }
    impl StreamWithSize for Input {
        fn get_size(&self) -> i64 {
            self.bytes.len() as i64
        }
    }
    impl SeekableReadStream for Input {
        fn read_block_at_offset(&self, buffer: &mut [u8], offset: i64) -> bool {
            if !self.in_callback.replace(true) {
                let callback = self.callback.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
                self.in_callback.set(false);
            }
            let Ok(start) = usize::try_from(offset) else {
                return false;
            };
            let Some(data) = start
                .checked_add(buffer.len())
                .and_then(|end| self.bytes.get(start..end))
            else {
                return false;
            };
            buffer.copy_from_slice(data);
            true
        }
    }
    let input = Rc::new(Input {
        bytes: std::fs::read(format!(
            "{}/../../upstream/pdfium/testing/resources/hello_world.pdf",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
        callback: RefCell::new(None),
        in_callback: Cell::new(false),
    });
    let document = Rc::new(RefCell::new(CpdfDocument::new()));
    assert_eq!(
        document.borrow_mut().load_doc(input.clone(), b""),
        crate::fpdfapi::parser::cpdf_parser::ParserError::Success
    );
    let environment = CpdfSdkFormFillEnvironment::new(&document, None);
    let runtime = environment.get_or_create_runtime().unwrap();
    let weak_document = Rc::downgrade(&document);
    let weak_runtime = Rc::downgrade(&runtime);
    let reads = Rc::new(Cell::new(0));
    let observed = reads.clone();
    *input.callback.borrow_mut() = Some(Rc::new(move || {
        let document = weak_document.upgrade().unwrap();
        assert_eq!(document.borrow().page_count(), 1);
        let runtime = weak_runtime.upgrade().unwrap();
        let value = runtime
            .engine()
            .eval(&script("globalThis.wordReadReentered=true;numPages"))
            .unwrap();
        assert_eq!(value.as_number(), Some(1.0));
        observed.set(observed.get() + 1);
    }));
    runtime.run_document_script(&[],&script("if(!(getPageNumWords(0)>0))throw Error('missing real file words');if(!wordReadReentered)throw Error('word content did not read');")).unwrap();
    assert!(reads.get() > 0);
}
