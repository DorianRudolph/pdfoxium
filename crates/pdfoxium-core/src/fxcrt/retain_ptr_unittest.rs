// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/retain_ptr_unittest.cpp over instrumented real Rc handles.
//! Const conversions share immutable Rc payloads; null/moved-from slots use
//! Option. Leak/Unleak transfer an owned token safely, without a raw leak.
#![allow(non_snake_case, non_upper_case_globals)]
use super::ownership_test_support::{PseudoRetainable, RetainPtr};

#[test]
fn DefaultCtor() {
    let ptr = RetainPtr::default();
    assert!(!ptr.is_some());
}

#[test]
fn NullptrCtor() {
    let ptr = RetainPtr::default();
    assert!(!ptr.is_some());
}

#[test]
fn RawCtor() {
    let obj = PseudoRetainable::default();
    {
        let ptr = RetainPtr::new(&obj);
        assert_eq!(Some(&obj as *const _), ptr.Get());
        assert_eq!(1, obj.retain_count());
        assert_eq!(0, obj.release_count());
    }
    assert_eq!(1, obj.retain_count());
    assert_eq!(1, obj.release_count());
}

#[test]
fn CopyCtor() {
    let obj = PseudoRetainable::default();
    {
        let ptr1 = RetainPtr::new(&obj);
        {
            let ptr2 = ptr1.clone();
            std::hint::black_box(&ptr2);
            assert_eq!(2, obj.retain_count());
            assert_eq!(0, obj.release_count());
        }
        assert_eq!(2, obj.retain_count());
        assert_eq!(1, obj.release_count());
    }
    assert_eq!(2, obj.retain_count());
    assert_eq!(2, obj.release_count());
}

#[test]
fn MoveCtor() {
    let obj = PseudoRetainable::default();
    {
        let mut ptr1 = RetainPtr::new(&obj);
        {
            let ptr2 = std::mem::take(&mut ptr1);
            assert!(!ptr1.is_some());
            assert_eq!(Some(&obj as *const _), ptr2.Get());
            assert_eq!(1, obj.retain_count());
            assert_eq!(0, obj.release_count());
        }
        assert_eq!(1, obj.retain_count());
        assert_eq!(1, obj.release_count());
    }
    assert_eq!(1, obj.retain_count());
    assert_eq!(1, obj.release_count());
}

#[test]
fn CopyConversionCtor() {
    let obj = PseudoRetainable::default();
    {
        let ptr1 = RetainPtr::new(&obj);
        {
            let ptr2 = ptr1.clone();
            std::hint::black_box(&ptr2);
            assert_eq!(2, obj.retain_count());
            assert_eq!(0, obj.release_count());
        }
        assert_eq!(2, obj.retain_count());
        assert_eq!(1, obj.release_count());
    }
    assert_eq!(2, obj.retain_count());
    assert_eq!(2, obj.release_count());
}

#[test]
fn MoveConversionCtor() {
    let obj = PseudoRetainable::default();
    {
        let mut ptr1 = RetainPtr::new(&obj);
        {
            let ptr2 = std::mem::take(&mut ptr1);
            assert!(!ptr1.is_some());
            assert_eq!(Some(&obj as *const _), ptr2.Get());
            assert_eq!(1, obj.retain_count());
            assert_eq!(0, obj.release_count());
        }
        assert_eq!(1, obj.retain_count());
        assert_eq!(1, obj.release_count());
    }
    assert_eq!(1, obj.retain_count());
    assert_eq!(1, obj.release_count());
}

#[test]
fn NullptrAssign() {
    let obj = PseudoRetainable::default();
    let mut ptr = RetainPtr::new(&obj);
    ptr.Reset(None);
    assert!(!ptr.is_some());
}

#[test]
fn RawAssign() {
    let obj = PseudoRetainable::default();
    let ptr;
    ptr = RetainPtr::new(&obj);
    assert_eq!(Some(&obj as *const _), ptr.Get());
}

#[test]
fn CopyAssign() {
    let obj = PseudoRetainable::default();
    {
        let ptr = RetainPtr::new(&obj);
        {
            let mut ptr2 = RetainPtr::default();
            ptr2.assign(&ptr);
            assert_eq!(2, obj.retain_count());
            assert_eq!(0, obj.release_count());
        }
        {
            // Test assignment from wrapped underlying type.
            let ptr2;
            ptr2 = ptr.clone();
            std::hint::black_box(&ptr2);
            assert_eq!(3, obj.retain_count());
            assert_eq!(1, obj.release_count());
        }
        assert_eq!(3, obj.retain_count());
        assert_eq!(2, obj.release_count());
    }
    assert_eq!(3, obj.retain_count());
    assert_eq!(3, obj.release_count());
}

#[test]
fn MoveAssign() {
    let obj = PseudoRetainable::default();
    {
        let mut ptr1 = RetainPtr::new(&obj);
        {
            let mut ptr2 = RetainPtr::default();
            assert_eq!(Some(&obj as *const _), ptr1.Get());
            assert!(!ptr2.is_some());
            ptr2 = std::mem::take(&mut ptr1);
            assert!(!ptr1.is_some());
            assert_eq!(Some(&obj as *const _), ptr2.Get());
            assert_eq!(1, obj.retain_count());
            assert_eq!(0, obj.release_count());
        }
        assert_eq!(1, obj.retain_count());
        assert_eq!(1, obj.release_count());
    }
    assert_eq!(1, obj.retain_count());
    assert_eq!(1, obj.release_count());
}

#[test]
fn CopyConvertAssign() {
    let obj = PseudoRetainable::default();
    {
        let ptr = RetainPtr::new(&obj);
        {
            let mut ptr2 = RetainPtr::default();
            ptr2.assign(&ptr);
            assert_eq!(2, obj.retain_count());
            assert_eq!(0, obj.release_count());
        }
        {
            // Test assignment from wrapped underlying type.
            let ptr2;
            ptr2 = ptr.clone();
            std::hint::black_box(&ptr2);
            assert_eq!(3, obj.retain_count());
            assert_eq!(1, obj.release_count());
        }
        assert_eq!(3, obj.retain_count());
        assert_eq!(2, obj.release_count());
    }
    assert_eq!(3, obj.retain_count());
    assert_eq!(3, obj.release_count());
}

#[test]
fn MoveConvertAssign() {
    let obj = PseudoRetainable::default();
    {
        let mut ptr1 = RetainPtr::new(&obj);
        {
            let ptr2;
            ptr2 = std::mem::take(&mut ptr1);
            assert!(!ptr1.is_some());
            assert_eq!(Some(&obj as *const _), ptr2.Get());
            assert_eq!(1, obj.retain_count());
            assert_eq!(0, obj.release_count());
        }
        assert_eq!(1, obj.retain_count());
        assert_eq!(1, obj.release_count());
    }
    assert_eq!(1, obj.retain_count());
    assert_eq!(1, obj.release_count());
}

#[test]
fn ResetNull() {
    let obj = PseudoRetainable::default();
    {
        let mut ptr = RetainPtr::new(&obj);
        ptr.Reset(None);
        assert_eq!(1, obj.retain_count());
        assert_eq!(1, obj.release_count());
    }
    assert_eq!(1, obj.retain_count());
    assert_eq!(1, obj.release_count());
}

#[test]
fn Reset() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    {
        let mut ptr = RetainPtr::new(&obj1);
        ptr.Reset(Some(&obj2));
        assert_eq!(1, obj1.retain_count());
        assert_eq!(1, obj1.release_count());
        assert_eq!(1, obj2.retain_count());
        assert_eq!(0, obj2.release_count());
    }
    assert_eq!(1, obj1.retain_count());
    assert_eq!(1, obj1.release_count());
    assert_eq!(1, obj2.retain_count());
    assert_eq!(1, obj2.release_count());
}

#[test]
fn Swap() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    {
        let mut ptr1 = RetainPtr::new(&obj1);
        {
            let mut ptr2 = RetainPtr::new(&obj2);
            ptr1.Swap(&mut ptr2);
            assert_eq!(1, obj1.retain_count());
            assert_eq!(0, obj1.release_count());
            assert_eq!(1, obj2.retain_count());
            assert_eq!(0, obj2.release_count());
        }
        assert_eq!(1, obj1.retain_count());
        assert_eq!(1, obj1.release_count());
        assert_eq!(1, obj2.retain_count());
        assert_eq!(0, obj2.release_count());
    }
    assert_eq!(1, obj1.retain_count());
    assert_eq!(1, obj1.release_count());
    assert_eq!(1, obj2.retain_count());
    assert_eq!(1, obj2.release_count());
}

#[test]
fn Leak() {
    let obj = PseudoRetainable::default();
    let leak;
    {
        let mut ptr = RetainPtr::new(&obj);
        leak = ptr.Leak();
        assert_eq!(1, obj.retain_count());
        assert_eq!(0, obj.release_count());
    }
    assert_eq!(1, obj.retain_count());
    assert_eq!(0, obj.release_count());
    {
        let mut ptr = RetainPtr::default();
        ptr.Unleak(leak);
        assert_eq!(1, obj.retain_count());
        assert_eq!(0, obj.release_count());
    }
    assert_eq!(1, obj.retain_count());
    assert_eq!(1, obj.release_count());
}

#[test]
fn SwapNull() {
    let obj1 = PseudoRetainable::default();
    {
        let mut ptr1 = RetainPtr::new(&obj1);
        {
            let mut ptr2 = RetainPtr::default();
            ptr1.Swap(&mut ptr2);
            assert!(!ptr1.is_some());
            assert!(ptr2.is_some());
            assert_eq!(1, obj1.retain_count());
            assert_eq!(0, obj1.release_count());
        }
        assert_eq!(1, obj1.retain_count());
        assert_eq!(1, obj1.release_count());
    }
    assert_eq!(1, obj1.retain_count());
    assert_eq!(1, obj1.release_count());
}

#[test]
fn Equals() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    let null_ptr1 = RetainPtr::default();
    let obj1_ptr1 = RetainPtr::new(&obj1);
    let obj2_ptr1 = RetainPtr::new(&obj2);
    {
        let null_ptr2 = RetainPtr::default();
        assert!(null_ptr1 == null_ptr2);
        assert!(null_ptr1.Get() == None);

        let obj1_ptr2 = RetainPtr::new(&obj1);
        assert!(obj1_ptr1 == obj1_ptr2);
        assert!((obj1_ptr2.Get() == Some(&obj1 as *const _)));

        let obj2_ptr2 = RetainPtr::new(&obj2);
        assert!(obj2_ptr1 == obj2_ptr2);
    }
    assert!(!(null_ptr1 == obj1_ptr1));
    assert!(!(null_ptr1 == obj2_ptr1));
    assert!(!(obj1_ptr1 == obj2_ptr1));
}

#[test]
fn EqualsReflexive() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    let obj1_ptr = RetainPtr::new(&obj1);
    let obj2_ptr = RetainPtr::new(&obj2);
    assert!((Some(&obj1 as *const _) == obj1_ptr.Get()));
    assert!(!(Some(&obj1 as *const _) == obj2_ptr.Get()));
    assert!(!(Some(&obj2 as *const _) == obj1_ptr.Get()));
    assert!((Some(&obj2 as *const _) == obj2_ptr.Get()));
}

#[test]
fn NotEquals() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    let null_ptr1 = RetainPtr::default();
    let obj1_ptr1 = RetainPtr::new(&obj1);
    let obj2_ptr1 = RetainPtr::new(&obj2);
    {
        let null_ptr2 = RetainPtr::default();
        let obj1_ptr2 = RetainPtr::new(&obj1);
        let obj2_ptr2 = RetainPtr::new(&obj2);
        assert!(!(null_ptr1 != null_ptr2));
        assert!(!(obj1_ptr1 != obj1_ptr2));
        assert!(!(obj2_ptr1 != obj2_ptr2));
    }
    assert!(null_ptr1 != obj1_ptr1);
    assert!(null_ptr1 != obj2_ptr1);
    assert!(obj1_ptr1 != obj2_ptr1);
}

#[test]
fn NotEqualsReflexive() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    let obj1_ptr = RetainPtr::new(&obj1);
    let obj2_ptr = RetainPtr::new(&obj2);
    assert!(!(Some(&obj1 as *const _) != obj1_ptr.Get()));
    assert!((Some(&obj1 as *const _) != obj2_ptr.Get()));
    assert!((Some(&obj2 as *const _) != obj1_ptr.Get()));
    assert!(!(Some(&obj2 as *const _) != obj2_ptr.Get()));
}

#[test]
fn LessThan() {
    let objs = [PseudoRetainable::default(), PseudoRetainable::default()];
    let obj1_ptr = RetainPtr::new(&objs[0]);
    let obj2_ptr = RetainPtr::new(&objs[1]);
    assert!(obj1_ptr < obj2_ptr);
    assert!(!(obj2_ptr < obj1_ptr));
}

#[test]
fn Bool() {
    let obj1 = PseudoRetainable::default();
    let null_ptr = RetainPtr::default();
    let obj1_ptr = RetainPtr::new(&obj1);
    let null_bool = null_ptr.is_some();
    let obj1_bool = obj1_ptr.is_some();
    assert!(!null_bool);
    assert!(obj1_bool);
}

#[test]
fn VectorMove() {
    // Proves move ctor is selected by std::vector over copy/delete, this
    // may require the ctor to be marked "noexcept".
    let obj = PseudoRetainable::default();
    {
        let mut ptr = RetainPtr::new(&obj);
        let mut vec1 = Vec::<RetainPtr>::new();
        vec1.push(std::mem::take(&mut ptr));
        assert_eq!(1, obj.retain_count());
        assert_eq!(0, obj.release_count());

        let mut vec2 = std::mem::take(&mut vec1);
        assert_eq!(1, obj.retain_count());
        assert_eq!(0, obj.release_count());

        vec2.resize_with(4096, RetainPtr::default);
        assert_eq!(1, obj.retain_count());
        assert_eq!(0, obj.release_count());
    }
    assert_eq!(1, obj.retain_count());
    assert_eq!(1, obj.release_count());
}

#[test]
fn AmbiguousExpression() {
    trait A {}
    struct Base;
    struct B;
    impl A for Base {}
    impl A for B {}
    let var: std::rc::Rc<dyn A> = if false {
        std::rc::Rc::new(Base)
    } else {
        std::rc::Rc::new(B)
    };
    assert!(std::rc::Rc::strong_count(&var) > 0);
}
#[test]
fn MakeRetained() {
    let ptr = std::rc::Rc::new(());
    assert_eq!(std::rc::Rc::strong_count(&ptr), 1);
    {
        let _other = std::rc::Rc::clone(&ptr);
        assert_ne!(std::rc::Rc::strong_count(&ptr), 1);
    }
    assert_eq!(std::rc::Rc::strong_count(&ptr), 1);
}
#[test]
fn SetContains() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    let ptr1 = RetainPtr::new(&obj1);
    let ptr2 = RetainPtr::new(&obj2);
    let const_ptr1 = RetainPtr::new(&obj1);
    let const_ptr2 = RetainPtr::new(&obj2);
    // Indexed identity keys enable heterogeneous find without linear search
    // or constructing temporary owning pointers, as in native std::less<>.
    let mut the_set = std::collections::BTreeMap::new();
    assert_eq!(2, obj1.retain_count());
    assert_eq!(0, obj1.release_count());
    assert_eq!(2, obj2.retain_count());
    assert_eq!(0, obj2.release_count());
    the_set.insert(const_ptr1.Get(), const_ptr1.clone());
    assert_eq!(3, obj1.retain_count());
    assert_eq!(0, obj1.release_count());
    assert_eq!(2, obj2.retain_count());
    assert_eq!(0, obj2.release_count());
    assert!(the_set.get(&Some(&obj1 as *const _)).is_some());
    assert!(the_set.get(&Some(&obj2 as *const _)).is_none());
    assert!(the_set.contains_key(&Some(&obj1 as *const _)));
    assert!(!the_set.contains_key(&Some(&obj2 as *const _)));
    assert_eq!(3, obj1.retain_count());
    assert_eq!(0, obj1.release_count());
    assert_eq!(2, obj2.retain_count());
    assert_eq!(0, obj2.release_count());
    assert!(the_set.get(&const_ptr1.Get()).is_some());
    assert!(the_set.get(&const_ptr2.Get()).is_none());
    assert!(the_set.contains_key(&const_ptr1.Get()));
    assert!(!the_set.contains_key(&const_ptr2.Get()));
    assert_eq!(3, obj1.retain_count());
    assert_eq!(0, obj1.release_count());
    assert_eq!(2, obj2.retain_count());
    assert_eq!(0, obj2.release_count());
    assert!(the_set.get(&ptr1.Get()).is_some());
    assert!(the_set.get(&ptr2.Get()).is_none());
    assert!(the_set.contains_key(&ptr1.Get()));
    assert!(!the_set.contains_key(&ptr2.Get()));
    // Selected non-sanitizer branch. C++ sanitizer conversion churn is absent.
    const kExpectedObj2RetainCount: i32 = 2;
    const kExpectedObj2ReleaseCount: i32 = 0;
    assert_eq!(kExpectedObj2RetainCount, obj2.retain_count());
    assert_eq!(kExpectedObj2ReleaseCount, obj2.release_count());
}
#[test]
fn VectorContains() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    let vec = vec![Some(&obj1 as *const _)];
    assert!(vec.contains(&Some(&obj1 as *const _)));
    assert!(!vec.contains(&Some(&obj2 as *const _)));
    let ptr1 = RetainPtr::new(&obj1);
    let ptr2 = RetainPtr::new(&obj2);
    assert!(vec.contains(&ptr1.Get()));
    assert!(!vec.contains(&ptr2.Get()));
    let const_ptr1 = RetainPtr::new(&obj1);
    let const_ptr2 = RetainPtr::new(&obj2);
    assert!(vec.contains(&const_ptr1.Get()));
    assert!(!vec.contains(&const_ptr2.Get()));
}
