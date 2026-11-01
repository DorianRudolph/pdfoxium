// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `jbig2_document_context.{h,cpp}`. The safe codec owns the native two-entry
//! symbol dictionary list so symbols remain in their decoded Rust form.

pub type Jbig2DocumentContext = hayro_jbig2::PdfiumDocumentContext;
