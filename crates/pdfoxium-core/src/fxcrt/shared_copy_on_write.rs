// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/shared_copy_on_write.h` using `Rc::make_mut`.

use std::rc::Rc;

#[derive(Debug)]
pub struct SharedCopyOnWrite<T: Clone> {
    object: Option<Rc<T>>,
}

impl<T: Clone> Default for SharedCopyOnWrite<T> {
    fn default() -> Self {
        Self { object: None }
    }
}

impl<T: Clone> Clone for SharedCopyOnWrite<T> {
    fn clone(&self) -> Self {
        Self {
            object: self.object.clone(),
        }
    }
}

impl<T: Clone> PartialEq for SharedCopyOnWrite<T> {
    fn eq(&self, other: &Self) -> bool {
        match (&self.object, &other.object) {
            (None, None) => true,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl<T: Clone> Eq for SharedCopyOnWrite<T> {}

impl<T: Clone> SharedCopyOnWrite<T> {
    pub fn emplace(&mut self, value: T) -> &mut T {
        self.object = Some(Rc::new(value));
        Rc::get_mut(self.object.as_mut().expect("just emplaced")).expect("new reference")
    }

    pub fn set_null(&mut self) {
        self.object = None;
    }

    pub fn get_object(&self) -> Option<&T> {
        self.object.as_deref()
    }

    pub fn get_private_copy(&mut self, default: impl FnOnce() -> T) -> &mut T {
        if self.object.is_none() {
            self.object = Some(Rc::new(default()));
        }
        Rc::make_mut(self.object.as_mut().expect("created above"))
    }

    pub fn is_some(&self) -> bool {
        self.object.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::SharedCopyOnWrite;

    #[test]
    fn clone_shares_until_mutation_and_emplace_replaces() {
        let mut a = SharedCopyOnWrite::default();
        assert!(!a.is_some());
        assert_eq!(a, a.clone());
        a.emplace(vec![1]);
        let mut b = a.clone();
        assert_eq!(a, b);
        b.get_private_copy(Vec::new).push(2);
        assert_ne!(a, b);
        assert_eq!(a.get_object().unwrap(), &[1]);
        assert_eq!(b.get_object().unwrap(), &[1, 2]);
        b.emplace(vec![3]);
        assert_eq!(b.get_object().unwrap(), &[3]);
        b.set_null();
        assert!(!b.is_some());
    }
}

#[cfg(test)]
#[path = "shared_copy_on_write_unittest.rs"]
mod upstream_tests;
