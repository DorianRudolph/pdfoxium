// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Embedded page thumbnails from `fpdfsdk/fpdf_thumbnail.cpp`.
use crate::fpdfapi::{
    page::{
        cpdf_dib::LoadState,
        cpdf_image::{CpdfImage, CpdfImageLoadContext},
        cpdf_page::CpdfPage,
    },
    parser::{
        cpdf_dictionary::CpdfDictionary, cpdf_object::ObjectRef, cpdf_scalar_object::ObjectType,
        cpdf_stream::CpdfStream, cpdf_stream_acc::CpdfStreamAcc,
    },
};
use crate::fxge::dib::{cfx_dibbase::realize, cfx_dibitmap::CfxDibitmap};
fn thumbnail_stream(page: &CpdfPage) -> Option<ObjectRef> {
    let dictionary = page.dict();
    let borrowed = dictionary.borrow();
    let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
    if !dict.key_exist(b"Type") {
        return None;
    }
    dict.get_typed_direct_for(b"Thumb", ObjectType::Stream)
}
pub fn with_thumbnail_data<R>(
    page: &CpdfPage,
    decoded: bool,
    consume: impl FnOnce(&[u8]) -> R,
) -> Option<R> {
    let stream = thumbnail_stream(page)?;
    let borrowed = stream.borrow();
    let stream = borrowed.as_any().downcast_ref::<CpdfStream>()?;
    let mut accessor = CpdfStreamAcc::new(stream);
    if decoded {
        accessor.load_all_data_filtered();
    } else {
        accessor.load_all_data_raw();
    }
    Some(consume(accessor.get_span()))
}
pub fn thumbnail_as_bitmap(page: &CpdfPage) -> Option<CfxDibitmap> {
    let stream = thumbnail_stream(page)?;
    let resources = page.holder().page_resources();
    let context = CpdfImageLoadContext::new(page.page_data(), resources);
    let image = CpdfImage::from_stream_with_page_data(stream, false, page.page_data())?;
    let (mut source, mut state) = image.start_dib(context, false)?;
    while state == LoadState::Continue {
        state = source.continue_load(None);
    }
    if state != LoadState::Success {
        return None;
    }
    realize(source.as_ref())
}
