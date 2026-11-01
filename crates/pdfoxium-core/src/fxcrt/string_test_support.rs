// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016, 2024 The PDFium Authors
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Fixture adapters for native string tests over selected borrowed slices.
//! The presence flag distinguishes a null native view from a non-null empty
//! literal; all indexing, slicing, searching and comparison use actual slices.
#![allow(non_snake_case)]
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug)]
pub(super) struct View<'a, T> {
    data: &'a [T],
    present: bool,
}
impl<T> Default for View<'_, T> {
    fn default() -> Self {
        Self {
            data: &[],
            present: false,
        }
    }
}
impl<'a, T: Copy + Default + Ord> View<'a, T> {
    pub fn literal(data: &'a [T]) -> Self {
        let len = data
            .iter()
            .position(|&v| v == T::default())
            .unwrap_or(data.len());
        Self {
            data: &data[..len],
            present: true,
        }
    }
    pub fn explicit(data: &'a [T], len: usize) -> Self {
        Self {
            data: &data[..len],
            present: true,
        }
    }
    pub fn from_span(data: &'a [T]) -> Self {
        if data.is_empty() {
            Self::default()
        } else {
            Self {
                data,
                present: true,
            }
        }
    }
    pub fn from_ref(ch: &'a T) -> Self {
        Self {
            data: std::slice::from_ref(ch),
            present: true,
        }
    }
    pub fn span(&self) -> &'a [T] {
        self.data
    }
    pub fn has_data(&self) -> bool {
        self.present
    }
    pub fn GetLength(&self) -> usize {
        self.data.len()
    }
    pub fn IsEmpty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn Find(&self, ch: T) -> Option<usize> {
        self.data.iter().position(|&v| v == ch)
    }
    pub fn SubstrFrom(&self, offset: usize) -> Self {
        self.Substr(offset, self.data.len().wrapping_sub(offset))
    }
    pub fn Substr(&self, first: usize, count: usize) -> Self {
        if first >= self.data.len() || count == 0 || count > self.data.len() - first {
            return Self::default();
        }
        Self {
            data: &self.data[first..first + count],
            present: true,
        }
    }
    pub fn TrimmedRight(&self, ch: T) -> Self {
        let last = self
            .data
            .iter()
            .rposition(|&v| v != ch)
            .map_or(0, |i| i + 1);
        Self::from_span(&self.data[..last])
    }
}
impl<T: PartialEq> PartialEq for View<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}
impl<T: Eq> Eq for View<'_, T> {}
impl<T: Ord> PartialOrd for View<'_, T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T: Ord> Ord for View<'_, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.data.cmp(other.data)
    }
}
impl<T: PartialEq> PartialEq<Vec<T>> for View<'_, T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.data == other.as_slice()
    }
}
impl<T: PartialEq> PartialEq<View<'_, T>> for Vec<T> {
    fn eq(&self, other: &View<'_, T>) -> bool {
        self.as_slice() == other.data
    }
}
impl<T> std::ops::Index<usize> for View<'_, T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        &self.data[i]
    }
}
impl View<'_, u8> {
    pub fn GetID(&self) -> u32 {
        super::bytestring::get_id(self.data)
    }
}

// Compile-time UTF-32 literals preserve the native L"..." inputs and borrowed
// lifetimes. This is fixture construction, not an alternative engine decoder.
pub(super) const fn literal_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut len = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] & 0xc0 != 0x80 {
            len += 1;
        }
        i += 1;
    }
    len
}
pub(super) const fn literal_wide<const N: usize>(text: &str) -> [u32; N] {
    let bytes = text.as_bytes();
    let mut result = [0; N];
    let mut i = 0;
    let mut out = 0;
    while i < bytes.len() {
        let first = bytes[i];
        i += 1;
        let (mut cp, remaining) = if first < 0x80 {
            (first as u32, 0)
        } else if first < 0xe0 {
            ((first & 0x1f) as u32, 1)
        } else if first < 0xf0 {
            ((first & 0xf) as u32, 2)
        } else {
            ((first & 7) as u32, 3)
        };
        let mut j = 0;
        while j < remaining {
            cp = (cp << 6) | (bytes[i] & 0x3f) as u32;
            i += 1;
            j += 1;
        }
        result[out] = cp;
        out += 1;
    }
    result
}
macro_rules! w {
    ($text:literal) => {{
        const DATA: [u32; $crate::fxcrt::string_test_support::literal_len($text)] =
            $crate::fxcrt::string_test_support::literal_wide($text);
        &DATA
    }};
}
pub(super) use w;

pub(super) const fn const_substr<T>(data: &[T], first: usize, count: usize) -> &[T] {
    if first >= data.len() || count == 0 || count > data.len() - first {
        return &[];
    }
    data.split_at(first).1.split_at(count).0
}

impl<T: Ord> PartialOrd<Vec<T>> for View<'_, T> {
    fn partial_cmp(&self, rhs: &Vec<T>) -> Option<Ordering> {
        Some(self.data.cmp(rhs.as_slice()))
    }
}
impl<T: Ord> PartialOrd<View<'_, T>> for Vec<T> {
    fn partial_cmp(&self, rhs: &View<'_, T>) -> Option<Ordering> {
        Some(self.as_slice().cmp(rhs.data))
    }
}

// The selected Vec deletion primitive with the native invalid-range no-op.
pub(super) fn delete<T>(data: &mut Vec<T>, index: usize, count: usize) -> usize {
    if let Some(end) = index.checked_add(count) {
        if count > 0 && index <= data.len() && end <= data.len() {
            data.drain(index..end);
        }
    }
    data.len()
}

// Vec/slice adapters for the native mutation fixtures. These expose the same
// operations as the selected standard storage, with the native invalid-range
// guards. All native case-conversion inputs here are ASCII; byte conversion
// invokes the actual selected C-string helpers. Wide conversion states that
// fixture domain explicitly rather than pretending to reproduce ICU tables.
pub(super) trait TestUnit: Copy + Default + Ord + std::fmt::Debug + 'static {
    fn lower_fixture(text: &mut [Self]);
    fn upper_fixture(text: &mut [Self]);
    fn whitespace() -> &'static [Self];
}
impl TestUnit for u8 {
    fn lower_fixture(text: &mut [Self]) {
        super::fx_system::strlwr(text);
    }
    fn upper_fixture(text: &mut [Self]) {
        super::fx_system::strupr(text);
    }
    fn whitespace() -> &'static [Self] {
        b"\x09\x0a\x0b\x0c\x0d\x20"
    }
}
impl TestUnit for u32 {
    fn lower_fixture(text: &mut [Self]) {
        super::widestring::make_lower(text);
    }
    fn upper_fixture(text: &mut [Self]) {
        super::widestring::make_upper(text);
    }
    fn whitespace() -> &'static [Self] {
        &[9, 10, 11, 12, 13, 32]
    }
}

pub(super) trait VecStringOps<T: TestUnit> {
    fn Concat(&mut self, src: &[T], count: usize);
    fn Remove(&mut self, ch: T) -> usize;
    fn Replace(&mut self, old: &[T], new: &[T]) -> usize;
    fn Insert(&mut self, index: usize, ch: T) -> usize;
    fn InsertAtFront(&mut self, ch: T) -> usize;
    fn InsertAtBack(&mut self, ch: T) -> usize;
    fn Delete(&mut self, index: usize, count: usize) -> usize;
    fn SubstrFrom(&self, offset: usize) -> Vec<T>;
    fn Substr(&self, first: usize, count: usize) -> Vec<T>;
    fn First(&self, count: usize) -> Vec<T>;
    fn Last(&self, count: usize) -> Vec<T>;
    fn Find(&self, needle: &[T]) -> Option<usize>;
    fn FindFrom(&self, needle: &[T], start: usize) -> Option<usize>;
    fn ReverseFind(&self, ch: T) -> Option<usize>;
    fn MakeLower(&mut self);
    fn MakeUpper(&mut self);
    fn Trim(&mut self, targets: &[T]);
    fn TrimFront(&mut self, targets: &[T]);
    fn TrimBack(&mut self, targets: &[T]);
    fn TrimWhitespace(&mut self);
    fn TrimWhitespaceFront(&mut self);
    fn TrimWhitespaceBack(&mut self);
    fn GetBuffer(&mut self, min_length: usize) -> &mut [T];
    fn GetStringLength(&self) -> usize;
    fn ReleaseBuffer(&mut self, length: usize);
}
impl<T: TestUnit> VecStringOps<T> for Vec<T> {
    fn Concat(&mut self, src: &[T], count: usize) {
        let src = &src[..count];
        if src.is_empty() {
            return;
        }
        if self.capacity() < self.len() + src.len() {
            // Native grows by at least 50%. Allocate before releasing old
            // storage, preserving its observable data-pointer relocation.
            let mut next =
                Vec::with_capacity(self.len() + std::cmp::max(self.len() / 2, src.len()));
            next.extend_from_slice(self);
            next.extend_from_slice(src);
            *self = next;
        } else {
            self.extend_from_slice(src);
        }
    }
    fn Remove(&mut self, ch: T) -> usize {
        let old = self.len();
        self.retain(|&c| c != ch);
        old - self.len()
    }
    fn Replace(&mut self, old: &[T], new: &[T]) -> usize {
        if old.is_empty() {
            return 0;
        }
        let mut search = self.as_slice();
        let mut count = 0;
        while let Some(found) = search.windows(old.len()).position(|part| part == old) {
            count += 1;
            search = &search[found + old.len()..];
        }
        if count == 0 {
            return 0;
        }
        let new_length = self.len() - count * old.len() + count * new.len();
        if new_length == 0 {
            self.clear();
            return count;
        }
        let mut result = Vec::with_capacity(new_length);
        let mut search = self.as_slice();
        for _ in 0..count {
            let found = search
                .windows(old.len())
                .position(|part| part == old)
                .unwrap();
            result.extend_from_slice(&search[..found]);
            result.extend_from_slice(new);
            search = &search[found + old.len()..];
        }
        result.extend_from_slice(search);
        *self = result;
        count
    }
    fn Insert(&mut self, index: usize, ch: T) -> usize {
        if index <= self.len() {
            self.insert(index, ch);
        }
        self.len()
    }
    fn InsertAtFront(&mut self, ch: T) -> usize {
        self.Insert(0, ch)
    }
    fn InsertAtBack(&mut self, ch: T) -> usize {
        self.Insert(self.len(), ch)
    }
    fn Delete(&mut self, index: usize, count: usize) -> usize {
        delete(self, index, count)
    }
    fn SubstrFrom(&self, offset: usize) -> Vec<T> {
        self.Substr(offset, self.len().wrapping_sub(offset))
    }
    fn Substr(&self, first: usize, count: usize) -> Vec<T> {
        View::from_span(self).Substr(first, count).span().to_vec()
    }
    fn First(&self, count: usize) -> Vec<T> {
        self.Substr(0, count)
    }
    fn Last(&self, count: usize) -> Vec<T> {
        self.Substr(self.len().wrapping_sub(count), count)
    }
    fn Find(&self, needle: &[T]) -> Option<usize> {
        self.FindFrom(needle, 0)
    }
    fn FindFrom(&self, needle: &[T], start: usize) -> Option<usize> {
        if start >= self.len() || needle.is_empty() {
            return None;
        }
        self[start..]
            .windows(needle.len())
            .position(|part| part == needle)
            .map(|pos| start + pos)
    }
    fn ReverseFind(&self, ch: T) -> Option<usize> {
        self.iter().rposition(|&c| c == ch)
    }
    fn MakeLower(&mut self) {
        T::lower_fixture(self);
    }
    fn MakeUpper(&mut self) {
        T::upper_fixture(self);
    }
    fn Trim(&mut self, targets: &[T]) {
        self.TrimFront(targets);
        self.TrimBack(targets);
    }
    fn TrimFront(&mut self, targets: &[T]) {
        let count = self
            .iter()
            .position(|c| !targets.contains(c))
            .unwrap_or(self.len());
        if count > 0 {
            self.drain(..count);
        }
    }
    fn TrimBack(&mut self, targets: &[T]) {
        let end = self
            .iter()
            .rposition(|c| !targets.contains(c))
            .map_or(0, |i| i + 1);
        self.truncate(end);
    }
    fn TrimWhitespace(&mut self) {
        self.TrimWhitespaceBack();
        self.TrimWhitespaceFront();
    }
    fn TrimWhitespaceFront(&mut self) {
        self.TrimFront(T::whitespace());
    }
    fn TrimWhitespaceBack(&mut self) {
        self.TrimBack(T::whitespace());
    }
    fn GetBuffer(&mut self, min_length: usize) -> &mut [T] {
        if self.len() < min_length {
            self.resize(min_length, T::default());
        }
        self.as_mut_slice()
    }
    fn GetStringLength(&self) -> usize {
        self.iter()
            .position(|&c| c == T::default())
            .unwrap_or(self.len())
    }
    fn ReleaseBuffer(&mut self, length: usize) {
        let length = std::cmp::min(length, self.len());
        if length == 0 {
            self.clear();
            return;
        }
        // Native ReleaseBuffer relocates when at least 32 units are unused.
        if self.capacity() - length >= 32 {
            let next = self[..length].to_vec(); // Explicit native relocation.
            *self = next;
        } else {
            self.truncate(length);
        }
    }
}
// Rc/Vec storage for native copy-on-write/allocation fixtures. Reference counts
// and pointer changes are observations of real Rust owners and allocations.
// A terminated borrowed view adds the native C-string boundary to logical Vec
// storage without copying a buffer or making invalid references.
use std::rc::Rc;
#[derive(Clone, Debug)]
pub(super) struct SharedText<T: TestUnit> {
    data: Option<Rc<Vec<T>>>,
    zero: T,
}
impl<T: TestUnit> Default for SharedText<T> {
    fn default() -> Self {
        Self {
            data: None,
            zero: T::default(),
        }
    }
}
impl<T: TestUnit> SharedText<T> {
    pub fn new(literal: &[T]) -> Self {
        Self::from_span(View::literal(literal).span())
    }
    pub fn from_span(data: &[T]) -> Self {
        if data.is_empty() {
            Self::default()
        } else {
            Self {
                data: Some(Rc::new(data.to_vec())),
                zero: T::default(),
            }
        }
    }
    pub fn span(&self) -> &[T] {
        self.data.as_ref().map_or(&[], |data| data.as_slice())
    }
    pub fn unsigned_span(&self) -> &[T] {
        self.span()
    }
    pub fn span_with_terminator(&self) -> Terminated<'_, T> {
        Terminated {
            data: self.span(),
            zero: T::default(),
        }
    }
    pub fn unsigned_span_with_terminator(&self) -> Terminated<'_, T> {
        self.span_with_terminator()
    }
    pub fn c_str(&self) -> *const T {
        self.data
            .as_ref()
            .map_or(std::ptr::from_ref(&self.zero), |data| data.as_ptr())
    }
    pub fn IsEmpty(&self) -> bool {
        self.span().is_empty()
    }
    pub fn ReferenceCountForTesting(&self) -> usize {
        self.data.as_ref().map_or(0, Rc::strong_count)
    }
    pub fn clear(&mut self) {
        if let Some(data) = &mut self.data {
            if Rc::strong_count(data) == 1 {
                Rc::get_mut(data).unwrap().clear();
                return;
            }
        }
        self.data = None;
    }
    pub fn assign_span(&mut self, src: &[T]) {
        if src.is_empty() {
            self.clear();
            return;
        }
        if let Some(data) = &mut self.data {
            if Rc::strong_count(data) == 1 && data.capacity() >= src.len() {
                let data = Rc::get_mut(data).unwrap();
                data.clear();
                data.extend_from_slice(src);
                return;
            }
        }
        self.data = Some(Rc::new(src.to_vec()));
    }
    pub fn assign_literal(&mut self, literal: &[T]) {
        self.assign_span(View::literal(literal).span());
    }
    pub fn SetAt(&mut self, index: usize, ch: T) {
        assert!(index < self.span().len());
        Rc::make_mut(self.data.as_mut().unwrap())[index] = ch;
    }
    pub fn Concat(&mut self, src: &[T], count: usize) {
        let src = &src[..count];
        if src.is_empty() {
            return;
        }
        let Some(data) = &mut self.data else {
            self.data = Some(Rc::new(src.to_vec()));
            return;
        };
        if Rc::strong_count(data) == 1 {
            Rc::get_mut(data).unwrap().Concat(src, src.len());
            return;
        }
        let mut next = Vec::with_capacity(data.len() + std::cmp::max(data.len() / 2, src.len()));
        next.extend_from_slice(data);
        next.extend_from_slice(src);
        self.data = Some(Rc::new(next));
    }
    pub fn Remove(&mut self, ch: T) {
        if self.span().contains(&ch) {
            Rc::make_mut(self.data.as_mut().unwrap()).retain(|&c| c != ch);
        }
    }
    pub fn TrimWhitespaceFront(&mut self) {
        let count = self
            .span()
            .iter()
            .position(|c| !T::whitespace().contains(c))
            .unwrap_or(self.span().len());
        if count > 0 {
            Rc::make_mut(self.data.as_mut().unwrap()).drain(..count);
        }
    }
    pub fn TrimWhitespaceBack(&mut self) {
        let end = self
            .span()
            .iter()
            .rposition(|c| !T::whitespace().contains(c))
            .map_or(0, |i| i + 1);
        if end < self.span().len() {
            Rc::make_mut(self.data.as_mut().unwrap()).truncate(end);
        }
    }
    pub fn Reserve(&mut self, length: usize) {
        if self.data.is_none() && length == 0 {
            return;
        }
        if let Some(data) = &self.data {
            if Rc::strong_count(data) > 1 {
                let mut next = Vec::with_capacity(std::cmp::max(length, data.len()));
                next.extend_from_slice(data);
                self.data = Some(Rc::new(next));
                return;
            }
        }
        let data = self.data.get_or_insert_with(|| Rc::new(Vec::new()));
        let data = Rc::get_mut(data).unwrap();
        data.reserve_exact(length.saturating_sub(data.len()));
    }
    pub fn ReleaseBuffer(&mut self, length: usize) {
        if let Some(data) = &mut self.data {
            Rc::get_mut(data)
                .expect("native ReleaseBuffer requires unique storage")
                .ReleaseBuffer(length);
        }
    }
}
impl<T: TestUnit> std::ops::Index<usize> for SharedText<T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        &self.span()[index]
    }
}
impl<T: TestUnit> PartialEq for SharedText<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.span() == rhs.span()
    }
}
impl<T: TestUnit> PartialEq<View<'_, T>> for SharedText<T> {
    fn eq(&self, rhs: &View<'_, T>) -> bool {
        self.span() == rhs.span()
    }
}
impl<T: TestUnit> PartialEq<SharedText<T>> for View<'_, T> {
    fn eq(&self, rhs: &SharedText<T>) -> bool {
        self.span() == rhs.span()
    }
}

pub(super) struct Terminated<'a, T> {
    data: &'a [T],
    zero: T,
}
impl<T> Terminated<'_, T> {
    pub fn len(&self) -> usize {
        self.data.len() + 1
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter().chain(std::iter::once(&self.zero))
    }
}
impl<T> std::ops::Index<usize> for Terminated<'_, T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        if index == self.data.len() {
            &self.zero
        } else {
            &self.data[index]
        }
    }
}

pub(super) fn plus<T: TestUnit>(a: &[T], b: &[T]) -> SharedText<T> {
    let mut result = Vec::with_capacity(a.len() + b.len());
    result.extend_from_slice(a);
    result.extend_from_slice(b);
    SharedText {
        data: Some(Rc::new(result)),
        zero: T::default(),
    }
}
