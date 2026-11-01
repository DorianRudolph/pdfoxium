// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fpdfapi/page/cpdf_indexedcs.{h,cpp}`.

use std::rc::Rc;

use super::cpdf_basedcs::CpdfBasedCs;
use super::cpdf_colorspace::{ColorSpaceFamily, ColorSpaceRef, CpdfColorSpace, CpdfColorSpaceData};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fxge::dib::fx_dib::FxRgbStruct;

#[derive(Clone, Copy)]
pub struct IndexedColorMinMax {
    pub min: f32,
    pub max: f32,
}

pub struct CpdfIndexedCs {
    based: CpdfBasedCs,
    max_index: i32,
    lookup_table: Vec<u8>,
    component_min_max: Vec<IndexedColorMinMax>,
}

impl CpdfIndexedCs {
    /// `load_base` is the document's guarded color-space resolver.
    pub fn load(
        array_object: &ObjectRef,
        load_base: impl FnOnce(&ObjectRef) -> Option<ColorSpaceRef>,
    ) -> Option<Self> {
        let base_object = {
            let borrowed = array_object.borrow();
            let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
            if array.size() < 4 {
                return None;
            }
            array.get_direct_object_at(1)?
        };
        if Rc::ptr_eq(&base_object, array_object) {
            return None;
        }
        let base_cs = load_base(&base_object)?;
        if matches!(
            base_cs.family(),
            ColorSpaceFamily::Indexed | ColorSpaceFamily::Pattern
        ) {
            return None;
        }
        let components = base_cs.component_count() as usize;
        assert!(components != 0);
        let component_min_max = (0..components)
            .map(|index| {
                let (_, min, max) = base_cs.get_default_value(index);
                IndexedColorMinMax {
                    min,
                    max: max - min,
                }
            })
            .collect();
        let (max_index, table_object) = {
            let borrowed = array_object.borrow();
            let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
            (
                array.get_integer_at(2).clamp(0, 255),
                array.get_direct_object_at(3)?,
            )
        };
        let lookup_table = match table_object.borrow().object_type() {
            ObjectType::String => {
                let borrowed = table_object.borrow();
                borrowed
                    .as_any()
                    .downcast_ref::<CpdfString>()?
                    .get_string()
                    .to_vec()
            }
            ObjectType::Stream => {
                let borrowed = table_object.borrow();
                let stream = borrowed.as_any().downcast_ref::<CpdfStream>()?;
                let mut acc = CpdfStreamAcc::new(stream);
                acc.load_all_data_filtered();
                acc.get_span().to_vec()
            }
            _ => Vec::new(),
        };
        let mut based = CpdfBasedCs::new(ColorSpaceFamily::Indexed);
        based.set_base_cs(Some(base_cs));
        based.data_mut().set_component_count(1);
        Some(Self {
            based,
            max_index,
            lookup_table,
            component_min_max,
        })
    }

    pub fn max_index(&self) -> i32 {
        self.max_index
    }
}

#[cfg(test)]
mod tests {
    use super::super::cpdf_colorspace::get_stock_cs_for_name;
    use super::*;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::{
        CpdfIndirectObjectHolder, HolderRef,
    };
    use crate::fpdfapi::parser::cpdf_object::{make_reference, retain_object};
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn indexed(table_as_stream: bool) -> (ObjectRef, Option<HolderRef>) {
        let mut array = CpdfArray::new();
        array.append_new_name(b"Indexed");
        array.append_new_name(b"DeviceRGB");
        array.append_new(CpdfScalarObject::number_from_signed(1));
        let holder = if table_as_stream {
            let holder = CpdfIndirectObjectHolder::new();
            let stream = holder
                .borrow_mut()
                .new_indirect_stream_from_bytes(&[255, 0, 0, 0, 0, 255]);
            array.append(make_reference(&stream, &holder));
            Some(holder)
        } else {
            array.append_new(CpdfString::from_bytes(None, &[255, 0, 0, 0, 0, 255]));
            None
        };
        (retain_object(array), holder)
    }

    #[test]
    fn indexed_rgb_uses_string_and_stream_tables() {
        for stream in [false, true] {
            let (object, _holder) = indexed(stream);
            let cs = CpdfIndexedCs::load(&object, |base| {
                get_stock_cs_for_name(&base.borrow().get_string())
            })
            .unwrap();
            assert_eq!(cs.max_index(), 1);
            assert_eq!(
                cs.get_rgb(&[0.0]),
                Some(FxRgbStruct {
                    red: 1.0,
                    green: 0.0,
                    blue: 0.0
                })
            );
            assert_eq!(
                cs.get_rgb(&[1.0]),
                Some(FxRgbStruct {
                    red: 0.0,
                    green: 0.0,
                    blue: 1.0
                })
            );
            assert_eq!(cs.get_rgb(&[2.0]), None);
            let mut bgr = [0; 6];
            cs.translate_image_line(&mut bgr, &[0, 1], 2, 2, 1, false);
            assert_eq!(bgr, [0, 0, 255, 255, 0, 0]);
        }
    }
}

impl CpdfColorSpace for CpdfIndexedCs {
    fn data(&self) -> &CpdfColorSpaceData {
        self.based.data()
    }

    fn as_indexed_cs(&self) -> Option<&CpdfIndexedCs> {
        Some(self)
    }

    fn enable_std_conversion(&self, enabled: bool) {
        self.based.enable_std_conversion(enabled);
    }

    fn get_rgb(&self, values: &[f32]) -> Option<FxRgbStruct<f32>> {
        let index = *values.first()? as i32;
        if index < 0 || index > self.max_index {
            return None;
        }
        let index = index as usize;
        let components = self.component_min_max.len();
        let offset = index.checked_mul(components)?;
        let end = offset.checked_add(components)?;
        let table = self.lookup_table.get(offset..end)?;
        let values: Vec<f32> = self
            .component_min_max
            .iter()
            .zip(table)
            .map(|(component, byte)| component.min + component.max * f32::from(*byte) / 255.0)
            .collect();
        self.based.base_cs()?.get_rgb(&values)
    }
}
