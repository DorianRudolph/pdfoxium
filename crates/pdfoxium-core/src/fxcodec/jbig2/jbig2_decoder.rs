// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Retained Jbig2Decoder Start/Continue and final external-canvas inversion.
use super::jbig2_document_context::Jbig2DocumentContext;
use crate::{
    fxcrt::pauseindicator_iface::PauseIndicatorIface, fxge::dib::cfx_dibitmap::CfxDibitmap,
};
use hayro_jbig2::{Decoder, PdfiumDecodeBoundary, PdfiumProgressiveDecoder};
use std::{cell::RefCell, rc::Weak};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeStatus {
    Continue,
    Finished,
    Error,
}
pub struct Jbig2DecodeJob {
    decoder: PdfiumProgressiveDecoder,
    started: bool,
}
impl Jbig2DecodeJob {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Some(Self {
            decoder: PdfiumProgressiveDecoder::new(width, height).ok()?,
            started: false,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn continue_decode(
        &mut self,
        source: &[u8],
        globals: Option<&[u8]>,
        destination: &mut CfxDibitmap,
        document: &Weak<RefCell<Jbig2DocumentContext>>,
        global_key: u64,
        mut pause: Option<&mut dyn PauseIndicatorIface>,
    ) -> DecodeStatus {
        if !self.started {
            self.started = true;
            destination.buffer_mut().fill(0);
            if pause
                .as_mut()
                .is_some_and(|owner| owner.need_to_pause_now())
            {
                return DecodeStatus::Continue;
            }
        }
        let row_budget = if pause.is_some() { 1 } else { u32::MAX };
        loop {
            // A job owns its arithmetic/page state. The shared document cache
            // lease ends before the real embedder callback can reenter it.
            let result = {
                let context = document.upgrade();
                let mut borrowed = context.as_ref().map(|context| context.borrow_mut());
                let mut detached = Jbig2DocumentContext::default();
                self.decoder.advance(
                    source,
                    globals,
                    borrowed.as_deref_mut().unwrap_or(&mut detached),
                    global_key,
                    row_budget,
                )
            };
            match result {
                Err(_) => return DecodeStatus::Error,
                Ok(PdfiumDecodeBoundary::Finished { default_pixel }) => {
                    let mut sink = CanvasSink {
                        bitmap: destination,
                        row: 0,
                        invalid: false,
                    };
                    if self.decoder.emit(&mut sink).is_err()
                        || sink.invalid
                        || sink.row != sink.bitmap.height()
                    {
                        return DecodeStatus::Error;
                    }
                    let width = sink.bitmap.width() as usize;
                    let used = width.div_ceil(8);
                    let background = if default_pixel { 0 } else { 255 };
                    for y in 0..sink.bitmap.height() {
                        let Some(row) = sink.bitmap.scanline_mut(y) else {
                            return DecodeStatus::Error;
                        };
                        if width % 8 != 0 {
                            let mask = 0xffu8 >> (width % 8);
                            row[used - 1] = (row[used - 1] & !mask) | (background & mask);
                        }
                        row[used..].fill(background);
                    }
                    return DecodeStatus::Finished;
                }
                Ok(_) => {
                    if self.decoder.can_pause()
                        && pause
                            .as_mut()
                            .is_some_and(|owner| owner.need_to_pause_now())
                    {
                        return DecodeStatus::Continue;
                    }
                }
            }
        }
    }
}
struct CanvasSink<'a> {
    bitmap: &'a mut CfxDibitmap,
    row: i32,
    invalid: bool,
}
impl Decoder for CanvasSink<'_> {
    fn push_packed_row(&mut self, words: &[u32], width: u32) -> bool {
        if width != self.bitmap.width() as u32 {
            self.invalid = true;
            return true;
        }
        let Some(row) = self.bitmap.scanline_mut(self.row) else {
            self.invalid = true;
            return true;
        };
        let used = (width as usize).div_ceil(8);
        for (index, byte) in row[..used].iter_mut().enumerate() {
            let Some(word) = words.get(index / 4) else {
                self.invalid = true;
                return true;
            };
            *byte = !word.to_be_bytes()[index % 4];
        }
        true
    }
    fn next_line(&mut self) {
        self.row += 1;
    }
    fn push_pixel(&mut self, _: bool) {
        self.invalid = true;
    }
    fn push_pixel_chunk(&mut self, _: bool, _: u32) {
        self.invalid = true;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::dib::fx_dib::FxdibFormat;
    use std::{cell::Cell, rc::Rc};
    struct Pause {
        context: Rc<RefCell<Jbig2DocumentContext>>,
        calls: Cell<usize>,
    }
    impl PauseIndicatorIface for Pause {
        fn need_to_pause_now(&mut self) -> bool {
            // Actual owner reentry must not encounter the codec's shared-cache
            // borrow; all row/segment work is complete before this callback.
            let _ = self.context.borrow_mut().symbol_dictionary_keys().count();
            self.calls.set(self.calls.get() + 1);
            self.calls.get() >= 4
        }
    }
    #[test]
    fn retained_decode_matches_uninterrupted_pixels_cache_keys_and_canvas_identity() {
        let source = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let mut expected = CfxDibitmap::create(1152, 720, FxdibFormat::OneBppRgb, 0).unwrap();
        let mut expected_context = Jbig2DocumentContext::default();
        assert!(crate::fxcodec::bilevel::decode_jbig2_with_context(
            source,
            &mut expected,
            Some(globals),
            &mut expected_context,
            7
        ));
        let mut actual = CfxDibitmap::create(1152, 720, FxdibFormat::OneBppRgb, 0).unwrap();
        actual.buffer_mut().fill(0);
        let address = actual.buffer().as_ptr();
        let context = Rc::new(RefCell::new(Jbig2DocumentContext::default()));
        let mut pause = Pause {
            context: context.clone(),
            calls: Cell::new(0),
        };
        let mut job = Jbig2DecodeJob::new(1152, 720).unwrap();
        let mut suspensions = 0;
        loop {
            let result = job.continue_decode(
                source,
                Some(globals),
                &mut actual,
                &Rc::downgrade(&context),
                7,
                Some(&mut pause),
            );
            assert_eq!(actual.buffer().as_ptr(), address);
            match result {
                DecodeStatus::Continue => {
                    suspensions += 1;
                    assert!(actual.buffer().iter().all(|&byte| byte == 0));
                }
                DecodeStatus::Finished => break,
                DecodeStatus::Error => panic!("retained fixture failed"),
            }
            assert!(suspensions < 5000);
        }
        assert!(suspensions >= 1);
        assert_eq!(actual.buffer(), expected.buffer());
        assert_eq!(
            context
                .borrow()
                .symbol_dictionary_keys()
                .collect::<Vec<_>>(),
            expected_context
                .symbol_dictionary_keys()
                .collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod row_tests {
    use super::*;
    use crate::fxge::dib::fx_dib::FxdibFormat;
    use std::{cell::Cell, rc::Rc};
    fn segment(source: &mut Vec<u8>, number: u32, kind: u8, bytes: &[u8]) {
        source.extend_from_slice(&number.to_be_bytes());
        source.extend_from_slice(&[kind, 0, 1]);
        source.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        source.extend_from_slice(bytes);
    }
    struct Pause {
        document: Rc<RefCell<Jbig2DocumentContext>>,
        calls: Cell<usize>,
    }
    impl PauseIndicatorIface for Pause {
        fn need_to_pause_now(&mut self) -> bool {
            let _ = self.document.borrow_mut().symbol_dictionary_count();
            self.calls.set(self.calls.get() + 1);
            self.calls.get() >= 4
        }
    }
    #[test]
    fn real_row_continuations_release_cache_borrow_and_preserve_unwritten_canvas() {
        let mut source = Vec::new();
        let mut page = Vec::new();
        page.extend_from_slice(&37u32.to_be_bytes());
        page.extend_from_slice(&5u32.to_be_bytes());
        page.extend_from_slice(&[0; 11]);
        segment(&mut source, 1, 48, &page);
        let mut region = Vec::new();
        region.extend_from_slice(&37u32.to_be_bytes());
        region.extend_from_slice(&5u32.to_be_bytes());
        region.extend_from_slice(&[0; 8]);
        region.extend_from_slice(&[2, 0, 3, 255, 253, 255, 2, 254, 254, 254]);
        region.extend_from_slice(&[0x55; 256]);
        segment(&mut source, 2, 38, &region);
        segment(&mut source, 3, 49, &[]);
        let mut expected = CfxDibitmap::create(37, 5, FxdibFormat::OneBppRgb, 0).unwrap();
        assert!(crate::fxcodec::bilevel::decode_jbig2_into(
            &source,
            &mut expected,
            None
        ));
        let mut actual = CfxDibitmap::create(37, 5, FxdibFormat::OneBppRgb, 0).unwrap();
        actual.buffer_mut().fill(0x66);
        let identity = actual.buffer().as_ptr();
        let document = Rc::new(RefCell::new(Jbig2DocumentContext::default()));
        let mut pause = Pause {
            document: document.clone(),
            calls: Cell::new(0),
        };
        let mut job = Jbig2DecodeJob::new(37, 5).unwrap();
        for row in 1..=5 {
            assert_eq!(
                job.continue_decode(
                    &source,
                    None,
                    &mut actual,
                    &Rc::downgrade(&document),
                    0,
                    Some(&mut pause)
                ),
                DecodeStatus::Continue
            );
            assert_eq!(job.decoder.completed_generic_rows(), Some(row));
            assert_eq!(actual.buffer().as_ptr(), identity);
            assert!(actual.buffer().iter().all(|&byte| byte == 0));
        }
        assert_eq!(
            job.continue_decode(
                &source,
                None,
                &mut actual,
                &Rc::downgrade(&document),
                0,
                None
            ),
            DecodeStatus::Finished
        );
        assert_eq!(actual.buffer(), expected.buffer());
    }
}
