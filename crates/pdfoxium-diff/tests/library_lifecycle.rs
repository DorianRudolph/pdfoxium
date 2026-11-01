//! Regressions for shared DSO ownership under the normal parallel test runner.
mod common;

use common::Api;
use std::panic::{catch_unwind, panic_any};
use std::thread;

fn fresh_document(api: &Api, width: f64) -> f32 {
    // SAFETY: handles stay on their library's owning thread and are closed in
    // page/document order. No handle or callback state escapes the scenario.
    unsafe {
        let document = api.FPDF_CreateNewDocument();
        assert!(!document.is_null());
        assert_eq!(api.FPDF_GetPageCount(document), 0);
        let page = api.FPDFPage_New(document, 0, width, 100.);
        assert!(!page.is_null());
        let actual = api.FPDF_GetPageWidthF(page);
        assert_eq!(api.FPDF_GetPageCount(document), 1);
        api.FPDF_ClosePage(page);
        api.FPDF_CloseDocument(document);
        actual
    }
}

#[test]
fn concurrent_callers_share_one_owner_and_keep_fresh_document_state() {
    let callers: Vec<_> = (0..8)
        .map(|index| {
            thread::spawn(move || {
                let caller = thread::current().id();
                let result = common::with_libraries(move |native, rust| {
                    let width = 100. + f64::from(index);
                    assert_eq!(fresh_document(native, width), width as f32);
                    assert_eq!(fresh_document(rust, width), width as f32);
                    (
                        thread::current().id(),
                        native.as_ref() as *const _ as *const () as usize,
                        rust.as_ref() as *const _ as *const () as usize,
                    )
                });
                assert_ne!(caller, result.0);
                result
            })
        })
        .collect();
    let results: Vec<_> = callers
        .into_iter()
        .map(|caller| caller.join().unwrap())
        .collect();
    assert!(results.iter().all(|result| result == &results[0]));
}

#[derive(Debug, PartialEq)]
struct ScenarioPanic(u32);

#[test]
fn scenario_panic_returns_to_caller_without_losing_the_library_session() {
    let before = common::with_libraries(|_, _| thread::current().id());
    let failure = catch_unwind(|| {
        common::with_libraries(|_, _| panic_any(ScenarioPanic(42)));
    })
    .unwrap_err();
    assert_eq!(
        *failure.downcast::<ScenarioPanic>().unwrap(),
        ScenarioPanic(42)
    );
    common::with_libraries(move |native, rust| {
        assert_eq!(thread::current().id(), before);
        assert_eq!(fresh_document(native, 123.), fresh_document(rust, 123.));
    });
}

#[test]
fn recursive_dispatch_fails_instead_of_deadlocking() {
    let failure = catch_unwind(|| {
        common::with_libraries(|_, _| common::with_libraries(|_, _| ()));
    })
    .unwrap_err();
    let message = failure.downcast::<String>().unwrap();
    assert!(message.contains("callbacks must reenter bindings directly"));
    common::with_libraries(|native, rust| {
        assert_eq!(fresh_document(native, 321.), fresh_document(rust, 321.));
    });
}
