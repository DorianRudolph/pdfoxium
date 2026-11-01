// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/pauseindicator_iface.h`.

pub trait PauseIndicatorIface {
    fn need_to_pause_now(&mut self) -> bool;
}
