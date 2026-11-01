// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use pdfoxium::{Document, RenderConfig};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pdfoxium-cli-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/render_corpus/pdfs")
        .join(name)
}
fn cli(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pdfoxium"))
        .args(args)
        .output()
        .unwrap()
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn info_text_search_and_password_commands() {
    let input = fixture("hello_world.pdf");
    let info = success(cli(&["info".as_ref(), input.as_os_str()]));
    assert!(info.contains("Pages: 1"));
    assert!(info.contains("Page 1:"));
    let text = success(cli(&["text".as_ref(), input.as_os_str()]));
    assert!(text.contains("Hello"));
    let search = success(cli(&[
        "search".as_ref(),
        input.as_os_str(),
        "Hello".as_ref(),
    ]));
    assert!(search.starts_with("1\t"));
    assert!(search.contains("\"Hello\""));
    let rects = success(cli(&[
        "search".as_ref(),
        input.as_os_str(),
        "Hello".as_ref(),
        "--rects".as_ref(),
    ]));
    assert!(rects.contains("Rect"));
    assert!(success(cli(&[
        "search".as_ref(),
        input.as_os_str(),
        "NOT_PRESENT_IN_PDF".as_ref()
    ]))
    .is_empty());
    let invalid = cli(&[
        "text".as_ref(),
        input.as_os_str(),
        "--pages".as_ref(),
        "999".as_ref(),
    ]);
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
    let encrypted = fixture("encrypted_r6.pdf");
    assert!(!cli(&[
        "text".as_ref(),
        encrypted.as_os_str(),
        "--password".as_ref(),
        "wrong".as_ref()
    ])
    .status
    .success());
    success(cli(&[
        "text".as_ref(),
        encrypted.as_os_str(),
        "--password".as_ref(),
        "âge".as_ref(),
    ]));
}

#[test]
fn split_preserves_order_duplicates_and_merge_reopens() {
    let tmp = Temp::new();
    let input = fixture("rectangles_multi_pages.pdf");
    let split = tmp.0.join("split.pdf");
    success(cli(&[
        "split".as_ref(),
        input.as_os_str(),
        "--pages".as_ref(),
        "3,1,3".as_ref(),
        "-o".as_ref(),
        split.as_os_str(),
    ]));
    let source = Document::open(&input).unwrap();
    let subset = Document::open(&split).unwrap();
    assert_eq!(subset.page_count().unwrap(), 3);
    let config = RenderConfig::fit(96, 96);
    for (index, original) in [2, 0, 2].into_iter().enumerate() {
        let expected = source.page(original).unwrap().render(&config).unwrap();
        let actual = subset.page(index).unwrap().render(&config).unwrap();
        assert_eq!(actual.pixels(), expected.pixels());
    }
    let merged = tmp.0.join("merged.pdf");
    success(cli(&[
        "merge".as_ref(),
        split.as_os_str(),
        input.as_os_str(),
        "--output".as_ref(),
        merged.as_os_str(),
    ]));
    let result = Document::open(&merged).unwrap();
    assert_eq!(
        result.page_count().unwrap(),
        3 + source.page_count().unwrap()
    );
    let expected = source.page(0).unwrap().render(&config).unwrap();
    assert_eq!(
        result.page(3).unwrap().render(&config).unwrap().pixels(),
        expected.pixels()
    );
}

#[test]
fn invalid_assembly_never_truncates_existing_output_or_input() {
    let tmp = Temp::new();
    let input = fixture("hello_world.pdf");
    let output = tmp.0.join("keep.pdf");
    std::fs::write(&output, b"existing output").unwrap();
    let bad = cli(&[
        "split".as_ref(),
        input.as_os_str(),
        "--pages".as_ref(),
        "1,999".as_ref(),
        "-o".as_ref(),
        output.as_os_str(),
    ]);
    assert!(!bad.status.success());
    assert_eq!(std::fs::read(&output).unwrap(), b"existing output");
    let missing = tmp.0.join("missing.pdf");
    assert!(!cli(&[
        "merge".as_ref(),
        input.as_os_str(),
        missing.as_os_str(),
        "-o".as_ref(),
        output.as_os_str()
    ])
    .status
    .success());
    assert_eq!(std::fs::read(&output).unwrap(), b"existing output");
    let local_input = tmp.0.join("input.pdf");
    let bytes = std::fs::read(input).unwrap();
    std::fs::write(&local_input, &bytes).unwrap();
    assert!(!cli(&[
        "split".as_ref(),
        local_input.as_os_str(),
        "--pages".as_ref(),
        "1".as_ref(),
        "-o".as_ref(),
        local_input.as_os_str()
    ])
    .status
    .success());
    assert_eq!(std::fs::read(local_input).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&tmp.0).unwrap().count(), 2);
}

#[test]
fn render_serial_and_parallel_outputs_match() {
    let tmp = Temp::new();
    let input = fixture("rectangles_multi_pages.pdf");
    let serial = tmp.0.join("serial");
    let parallel = tmp.0.join("parallel");
    for (output, jobs) in [(&serial, "1"), (&parallel, "4")] {
        success(cli(&[
            "render".as_ref(),
            input.as_os_str(),
            "--pages".as_ref(),
            "1-3".as_ref(),
            "--dpi".as_ref(),
            "36".as_ref(),
            "--jobs".as_ref(),
            jobs.as_ref(),
            "-o".as_ref(),
            output.as_os_str(),
        ]));
    }
    for index in 1..=3 {
        let name = format!("page-{index:04}.png");
        assert_eq!(
            std::fs::read(serial.join(&name)).unwrap(),
            std::fs::read(parallel.join(name)).unwrap()
        );
    }
}

#[test]
fn empty_query_is_rejected_for_an_empty_document() {
    let tmp = Temp::new();
    let input = tmp.0.join("empty.pdf");
    let mut file = std::fs::File::create(&input).unwrap();
    Document::create().unwrap().save_to(&mut file).unwrap();
    drop(file);
    let output = cli(&["search".as_ref(), input.as_os_str(), "".as_ref()]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
