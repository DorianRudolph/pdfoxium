// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Pushbutton icon stream properties from `core/fpdfdoc/cpdf_icon.{h,cpp}`.
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxSizeF};

pub struct CpdfIcon {
    stream: ObjectRef,
}
impl CpdfIcon {
    pub fn new(stream: ObjectRef) -> Self {
        Self { stream }
    }
    fn with_dict<R>(&self, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
        let dict = get_dict_object(&self.stream).expect("icon stream dictionary");
        let borrowed = dict.borrow();
        f(borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("icon dictionary"))
    }
    pub fn image_size(&self) -> CfxSizeF {
        self.with_dict(|dict| {
            let rect = dict.get_rect_for(b"BBox");
            CfxSizeF::new(rect.right - rect.left, rect.top - rect.bottom)
        })
    }
    pub fn image_matrix(&self) -> CfxMatrix {
        self.with_dict(|dict| dict.get_matrix_for(b"Matrix"))
    }
    pub fn image_alias(&self) -> Vec<u8> {
        self.with_dict(|dict| dict.get_byte_string_for(b"Name", b""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    #[test]
    fn reads_icon_geometry_matrix_and_alias() {
        let mut dict = CpdfDictionary::new();
        dict.set_rect_for(b"BBox", CfxFloatRect::new(10., 20., 40., 60.));
        let matrix = CfxMatrix::new(2., 0., 0., 3., 4., 5.);
        dict.set_matrix_for(b"Matrix", matrix);
        dict.set_new_name_for(b"Name", b"Icon1");
        let icon = CpdfIcon::new(retain_object(CpdfStream::from_dict(retain_object(dict))));
        let size = icon.image_size();
        assert_eq!((size.width, size.height), (30., 40.));
        assert_eq!(icon.image_matrix(), matrix);
        assert_eq!(icon.image_alias(), b"Icon1");
    }
}
