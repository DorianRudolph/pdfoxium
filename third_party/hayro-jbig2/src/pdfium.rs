// Copyright 2014 The PDFium Authors
// Use of this source code is governed by the BSD-style license in
// ../LICENSE-PDFIUM.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Document symbol cache from PDFium's jbig2_document_context and
//! CJBig2_Context::ParseSymbolDict. Deep copies here are native cache ownership,
//! not a workaround for borrowing the decoder's symbol references.

use crate::decode::symbol::SymbolDictionary;
use alloc::vec::Vec;

#[derive(Default)]
pub struct PdfiumDocumentContext {
    // Freshest first. PDFium deliberately keeps only two dictionaries.
    entries: Vec<((u64, u32), SymbolDictionary)>,
}
impl PdfiumDocumentContext {
    pub fn symbol_dictionary_count(&self) -> usize {
        self.entries.len()
    }
    pub fn symbol_dictionary_keys(&self) -> impl Iterator<Item = (u64, u32)> + '_ {
        self.entries.iter().map(|entry| entry.0)
    }

    pub(crate) fn get(&mut self, key: (u64, u32)) -> Option<SymbolDictionary> {
        if key.0 == 0 {
            return None;
        }
        let index = self.entries.iter().position(|entry| entry.0 == key)?;
        let entry = self.entries.remove(index);
        let dictionary = entry.1.clone();
        self.entries.insert(0, entry);
        Some(dictionary)
    }
    pub(crate) fn insert(&mut self, key: (u64, u32), dictionary: &SymbolDictionary) {
        // Native insertion precedes SetGbContexts/SetGrContexts. Only the
        // exported bitmaps have been installed at this point.
        let copy = SymbolDictionary {
            exported_symbols: dictionary.exported_symbols.clone(),
            retained_contexts: None,
        };
        while self.entries.len() >= 2 {
            self.entries.pop();
        }
        self.entries.insert(0, (key, copy));
    }
}
