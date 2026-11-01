// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT
use std::process::{Command, Output};
use std::{fs, path::Path};

fn run(input: &Path, report: &Path, options: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pdfoxium-diff"))
        .arg("check")
        .arg(input)
        .arg("--report")
        .arg(report)
        .args(options)
        .output()
        .expect("run comparison gate")
}
fn read(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn directory(name: &str) -> std::path::PathBuf {
    let path = pdfoxium_diff::project_root().join(format!("target/{name}-{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[cfg(debug_assertions)]
#[test]
fn benchmarks_refuse_debug_builds() {
    let result = Command::new(env!("CARGO_BIN_EXE_pdfoxium-diff"))
        .args(["bench", "blank"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("requires --release"));
}

#[test]
fn in_memory_comparison_ignores_old_pngs_and_writes_no_success_images() {
    let root = pdfoxium_diff::project_root();
    let dir = directory("in-memory-regression");
    let pdf = dir.join("input.pdf");
    fs::copy(root.join("tests/render_corpus/pdfs/blank.pdf"), &pdf).unwrap();
    let report = dir.join("report.json");
    let case = pdfoxium_diff::catalog::select_case(pdf.to_str().unwrap()).unwrap();
    let cache = root.join("target/render-diff/rust").join(case.run_id());
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("0000.png"), b"corrupt stale cache").unwrap();
    let output = run(&pdf, &report, &["--page", "0", "--flip"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = read(&report);
    let result = &json["cases"][&case.id];
    assert_eq!(result["dpi"], 144);
    assert_eq!(result["status"], "passed");
    assert_eq!(result["pages"][0]["score"], 0.);
    assert_eq!(result["pages"][0]["flip"]["max_mean_4x4"]["mean"], 0.);
    assert!(!report.with_extension("failures").exists());
    assert_eq!(
        fs::read(cache.join("0000.png")).unwrap(),
        b"corrupt stale cache"
    );
    let invalid = run(&pdf, &report, &["--page", "1"]);
    assert!(!invalid.status.success());
    assert!(fs::read_to_string(&report)
        .unwrap()
        .contains("page 1 out of range"));
    fs::remove_dir_all(cache).unwrap();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn directory_union_namespaces_collisions_and_continues_after_failure() {
    let root = pdfoxium_diff::project_root();
    let dir = directory("directory-regression");
    for subdir in ["a", "b"] {
        fs::create_dir_all(dir.join(subdir)).unwrap();
        fs::copy(
            root.join("tests/render_corpus/pdfs/blank.pdf"),
            dir.join(subdir).join("same,book.PDF"),
        )
        .unwrap();
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&dir, dir.join("loop")).unwrap();
    let selection = pdfoxium_diff::catalog::select(
        &[
            dir.to_str().unwrap().into(),
            dir.join("a/same,book.PDF").to_str().unwrap().into(),
        ],
        false,
    )
    .unwrap();
    assert_eq!(selection.cases.len(), 2);
    assert_ne!(selection.cases[0].id, selection.cases[1].id);
    let report = dir.join("results.json");
    let checked = run(&dir, &report, &[]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert_eq!(read(&report)["cases"].as_object().unwrap().len(), 2);
    fs::write(dir.join("0-broken.pdf"), b"invalid PDF").unwrap();
    let checked = run(&dir, &report, &[]);
    assert!(!checked.status.success());
    let json = read(&report);
    let cases = json["cases"].as_object().unwrap();
    assert_eq!(cases.len(), 3);
    assert_eq!(
        cases.values().filter(|c| c["status"] == "passed").count(),
        2
    );
    assert_eq!(
        cases.values().filter(|c| c["status"] == "failed").count(),
        1
    );
    // A rerun never accepts a previous worker's successful report.
    let checked = run(&dir, &report, &["--expect", "reject"]);
    assert!(!checked.status.success());
    assert_eq!(
        read(&report)["cases"]
            .as_object()
            .unwrap()
            .values()
            .filter(|c| c["status"] == "failed")
            .count(),
        2
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn negative_expectations_never_hide_workflow_errors() {
    let root = pdfoxium_diff::project_root();
    let dir = directory("negative-regression");
    let pdf = dir.join("broken.pdf");
    fs::write(&pdf, b"intentionally not a PDF").unwrap();
    let report = dir.join("report.json");
    for expectation in ["reject", "no-crash"] {
        let output = run(&pdf, &report, &["--expect", expectation]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json = read(&report);
        let case = json["cases"].as_object().unwrap().values().next().unwrap();
        assert_eq!(case["native_page_count"], 0);
        assert_eq!(case["rust_page_count"], 0);
    }
    let blank = root.join("tests/render_corpus/pdfs/blank.pdf");
    assert!(run(&blank, &report, &["--expect", "no-crash"])
        .status
        .success());
    assert!(!run(&blank, &report, &["--expect", "reject"])
        .status
        .success());
    assert!(fs::read_to_string(&report)
        .unwrap()
        .contains("expected rejection"));
    assert!(
        !run(&blank, &report, &["--expect", "no-crash", "--page", "9999"])
            .status
            .success()
    );
    assert!(fs::read_to_string(&report)
        .unwrap()
        .contains("out of range"));
    assert!(!run(&pdf, &report, &["--expect", "render"]).status.success());
    assert!(fs::read_to_string(&report)
        .unwrap()
        .contains("document rejected"));
    fs::remove_dir_all(dir).unwrap();
}

/// Run the automatic raster gate for every bundled fixture page.
#[test]
#[ignore = "renders all bundled fixtures; build pdfoxium-capi first"]
fn all_bundled_pdf_pages() {
    assert!(Command::new(env!("CARGO_BIN_EXE_pdfoxium-diff"))
        .args(["check", "bundled"])
        .status()
        .unwrap()
        .success());
}
macro_rules! corpus_group {
    ($name:ident,$selector:literal) => {
        #[test]
        #[ignore = "render corpus group; fetch GROUP first"]
        fn $name() {
            assert!(Command::new(env!("CARGO_BIN_EXE_pdfoxium-diff"))
                .current_dir(pdfoxium_diff::project_root())
                .args([
                    "check",
                    $selector,
                    "--report",
                    concat!("target/render-diff/reports/", stringify!($name), ".json")
                ])
                .status()
                .unwrap()
                .success());
        }
    };
}
corpus_group!(workspace, "workspace");
corpus_group!(books, "books");
corpus_group!(hayro_local, "hayro/local");
corpus_group!(hayro_custom, "hayro/custom");
corpus_group!(hayro_pdfjs, "hayro/pdfjs");
corpus_group!(hayro_pdfbox, "hayro/pdfbox");
corpus_group!(hayro_pdfium, "hayro/pdfium");
corpus_group!(hayro_corpus, "hayro/corpus");
