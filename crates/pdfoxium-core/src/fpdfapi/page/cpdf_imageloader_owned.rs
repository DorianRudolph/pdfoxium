// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Plane ownership after an incremental cache decode has finished. No page,
//! image-object or cache reference escapes the short loading step.
use super::*;
impl CpdfImageLoader<'static> {
    pub(crate) fn from_cached_planes(
        bitmap: Option<Rc<CpdfImageBitmap>>,
        mask: Option<Rc<CpdfImageBitmap>>,
        matte_color: u32,
    ) -> Self {
        Self {
            bitmap,
            mask,
            cached: true,
            matte_color,
            cache: None,
            image_object: None,
        }
    }
}
