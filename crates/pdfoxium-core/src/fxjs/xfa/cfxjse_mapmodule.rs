// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! cfxjse_mapmodule.{h,cpp}. One tagged entry replaces three synchronized maps.
//! Native u32 keys and the parser's typed element/attribute keys share the path.
use crate::xfa::fxfa::parser::cxfa_measurement::CxfaMeasurement;
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    rc::Rc,
};
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MapValue {
    Scalar(i32),
    Text(Rc<[u32]>),
    Measure(CxfaMeasurement),
}
#[derive(Clone, Debug)]
pub(crate) struct CfxJseMapModule<K = u32> {
    entries: BTreeMap<K, MapValue>,
}
impl<K> Default for CfxJseMapModule<K> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}
impl<K> Deref for CfxJseMapModule<K> {
    type Target = BTreeMap<K, MapValue>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}
impl<K> DerefMut for CfxJseMapModule<K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}
impl<K: Ord + Copy> CfxJseMapModule<K> {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn merge_data_from(&mut self, source: &Self) {
        self.entries.extend(
            source
                .entries
                .iter()
                .map(|(&key, value)| (key, value.clone())),
        );
    }
}
// Native named accessors support the ported test vectors. Production node
// storage uses tagged entries directly to retain shared strings across observer
// callbacks without allocating another copy in SetString.
#[cfg(test)]
impl<K: Ord + Copy> CfxJseMapModule<K> {
    pub(crate) fn set_value(&mut self, key: K, value: i32) {
        self.entries.insert(key, MapValue::Scalar(value));
    }
    pub(crate) fn set_string(&mut self, key: K, value: &[u32]) {
        self.entries.insert(key, MapValue::Text(Rc::from(value)));
    }
    pub(crate) fn set_measurement(&mut self, key: K, value: CxfaMeasurement) {
        self.entries.insert(key, MapValue::Measure(value));
    }
    pub(crate) fn get_value(&self, key: K) -> Option<i32> {
        match self.entries.get(&key) {
            Some(MapValue::Scalar(value)) => Some(*value),
            _ => None,
        }
    }
    pub(crate) fn get_string(&self, key: K) -> Option<Rc<[u32]>> {
        match self.entries.get(&key) {
            Some(MapValue::Text(value)) => Some(value.clone()),
            _ => None,
        }
    }
    pub(crate) fn get_measurement(&self, key: K) -> Option<CxfaMeasurement> {
        match self.entries.get(&key) {
            Some(MapValue::Measure(value)) => Some(*value),
            _ => None,
        }
    }
    pub(crate) fn has_key(&self, key: K) -> bool {
        self.entries.contains_key(&key)
    }
    pub(crate) fn remove_key(&mut self, key: K) {
        self.entries.remove(&key);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    #[test]
    fn empty_module() {
        let map = CfxJseMapModule::<u32>::new();
        for key in [1, 2, 3] {
            assert!(!map.has_key(key));
        }
        assert!(map.get_value(1).is_none());
        assert!(map.get_string(2).is_none());
        assert!(map.get_measurement(3).is_none());
    }
    #[test]
    fn insert_delete() {
        let mut map = CfxJseMapModule::<u32>::new();
        let text = wide("foo");
        let measure = CxfaMeasurement::from_string(&wide("1 pt"));
        map.set_value(100, 101);
        map.set_string(200, &text);
        map.set_measurement(300, measure);
        for key in [100, 200, 300] {
            assert!(map.has_key(key));
        }
        assert_eq!(map.get_value(100), Some(101));
        assert!(map.get_string(100).is_none());
        assert!(map.get_measurement(100).is_none());
        assert!(map.get_value(200).is_none());
        assert_eq!(map.get_string(200).unwrap().as_ref(), text);
        assert!(map.get_measurement(200).is_none());
        assert!(map.get_value(300).is_none());
        assert!(map.get_string(300).is_none());
        assert_eq!(map.get_measurement(300), Some(measure));
        for key in [100, 200, 300] {
            map.remove_key(key);
            assert!(!map.has_key(key));
        }
        assert!(map.get_value(100).is_none());
        assert!(map.get_string(200).is_none());
        assert!(map.get_measurement(300).is_none());
    }
    #[test]
    fn key_collision() {
        let mut map = CfxJseMapModule::<u32>::new();
        let text = wide("foo");
        let measure = CxfaMeasurement::from_string(&wide("1 pt"));
        map.set_value(100, 37);
        assert!(map.has_key(100));
        assert_eq!(map.get_value(100), Some(37));
        assert!(map.get_string(100).is_none());
        assert!(map.get_measurement(100).is_none());
        map.set_string(100, &text);
        assert!(map.has_key(100));
        assert!(map.get_value(100).is_none());
        assert_eq!(map.get_string(100).unwrap().as_ref(), text);
        assert!(map.get_measurement(100).is_none());
        map.set_measurement(100, measure);
        assert!(map.get_value(100).is_none());
        assert!(map.get_string(100).is_none());
        assert_eq!(map.get_measurement(100), Some(measure));
        map.set_value(100, 37);
        assert!(map.has_key(100));
        assert_eq!(map.get_value(100), Some(37));
        assert!(map.get_string(100).is_none());
        assert!(map.get_measurement(100).is_none());
    }
    #[test]
    fn merge_data() {
        let mut first = CfxJseMapModule::<u32>::new();
        let mut second = CfxJseMapModule::<u32>::new();
        let text = wide("foo");
        let pt = CxfaMeasurement::from_string(&wide("1 pt"));
        let mm = CxfaMeasurement::from_string(&wide("2 mm"));
        for key in [100, 101] {
            first.set_value(key, 42);
        }
        for key in [200, 201] {
            first.set_string(key, &text);
        }
        for key in [300, 301] {
            first.set_measurement(key, pt);
        }
        second.set_string(100, &text);
        second.set_measurement(200, mm);
        second.set_value(300, -1999);
        first.merge_data_from(&second);
        assert_eq!(first.get_string(100).unwrap().as_ref(), text);
        assert_eq!(first.get_value(101), Some(42));
        assert_eq!(
            first.get_measurement(200).unwrap().get_unit(),
            mm.get_unit()
        );
        assert_eq!(first.get_string(201).unwrap().as_ref(), text);
        assert_eq!(first.get_value(300), Some(-1999));
        assert_eq!(
            first.get_measurement(301).unwrap().get_unit(),
            pt.get_unit()
        );
        assert_eq!(second.get_string(100).unwrap().as_ref(), text);
        assert_eq!(
            second.get_measurement(200).unwrap().get_unit(),
            mm.get_unit()
        );
        assert_eq!(second.get_value(300), Some(-1999));
    }
}
