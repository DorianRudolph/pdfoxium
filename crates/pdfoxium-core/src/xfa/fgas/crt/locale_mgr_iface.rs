// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/crt/locale_mgr_iface.h`; shared locale handles replace raw pointers.
use super::locale_iface::LocaleIface;
use std::rc::Rc;

pub trait LocaleMgrIface {
    fn get_def_locale(&self) -> Option<Rc<dyn LocaleIface>>;
    fn get_locale_by_name(&self, name: &[u32]) -> Option<Rc<dyn LocaleIface>>;
    fn set_def_locale(&self, locale: Option<Rc<dyn LocaleIface>>);
}
