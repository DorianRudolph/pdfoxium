// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[path = "bench_report.rs"]
pub(super) mod report;

#[derive(Serialize, Deserialize)]
struct Sample {
    page: usize,
    iteration: usize,
    width: i32,
    height: i32,
    load_ms: f64,
    render_ms: f64,
    fill_ms: f64,
}

#[derive(Serialize, Deserialize)]
struct Document {
    backend: String,
    case: String,
    document_load_ms: Vec<f64>,
    samples: Vec<Sample>,
    page_count: Option<usize>,
    exclusions: Vec<Excluded>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Excluded {
    page: Option<usize>,
    reason: String,
}

#[derive(Clone, Copy, Default, Serialize)]
struct Timing {
    load_ms: f64,
    render_ms: f64,
    total_ms: f64,
    fill_ms: f64,
    load_fill_draw_ms: f64,
}

#[derive(Serialize)]
struct Row {
    case: String,
    page: usize,
    dpi: u32,
    width: i32,
    height: i32,
    native: Timing,
    rust: Timing,
}

fn directory() -> PathBuf {
    std::env::var_os("PDFOXIUM_BENCH_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| project_root().join("target/render-bench"))
}

fn report_path(backend: &str, case: &Case) -> PathBuf {
    report_path_at(&directory(), backend, case)
}

fn report_path_at(base: &Path, backend: &str, case: &Case) -> PathBuf {
    let name = case.page.map_or_else(
        || case.run_id(),
        |page| format!("{}-page-{page}", case.run_id()),
    );
    base.join(backend).join(format!("{name}.json"))
}

fn validate_input(case: &Case) -> Result<()> {
    let path = case.input_path();
    if !path.is_file() {
        return Err(format!(
            "benchmark input is missing or is not a file: {}",
            path.display()
        )
        .into());
    }
    // File access failures are harness errors, not declared PDF rejections.
    fs::File::open(path)?;
    Ok(())
}

pub(super) fn worker(
    backend: &str,
    case: &Case,
    iterations: usize,
    rules: &check::Rules,
) -> Result<()> {
    if cfg!(debug_assertions) || iterations == 0 {
        return Err("bench requires a release build and at least one measured iteration".into());
    }
    validate_input(case)?;
    let bindings = Pdfium::bind_to_library(library_path(backend))?;
    let mut report = Document {
        backend: backend.to_owned(),
        case: case.id.clone(),
        document_load_ms: Vec::new(),
        samples: Vec::new(),
        page_count: None,
        exclusions: Vec::new(),
    };
    // SAFETY: Each worker owns one initialized library. Documents and pages
    // remain live until their matching closes, including on render errors.
    unsafe {
        bindings.FPDF_InitLibrary();
        let result: Result<()> = (|| {
            super::font_extensions::configure_test_limits(backend)?;
            for iteration in 0..=iterations {
                let path = project_root().join(&case.path);
                let start = Instant::now();
                let document = bindings.FPDF_LoadDocument(
                    path.to_str().ok_or("PDF path is not UTF-8")?,
                    case.password.as_deref(),
                );
                let document_ms = start.elapsed().as_secs_f64() * 1_000.0;
                if document.is_null() {
                    if rules.expect == Some(Expectation::Reject) && rules.rejected_pages.is_empty()
                    {
                        if iteration != 0 {
                            report.document_load_ms.push(document_ms);
                        }
                        if iteration == 1 {
                            report.exclusions.push(Excluded {
                                page: None,
                                reason: rules.reason.clone().unwrap_or_default(),
                            });
                        }
                        continue;
                    }
                    return Err(format!("load failed: {}", bindings.FPDF_GetLastError()).into());
                }
                let result: Result<()> = (|| {
                    let count = bindings.FPDF_GetPageCount(document);
                    if rules.expect == Some(Expectation::Reject) && rules.rejected_pages.is_empty()
                    {
                        if count != 0 {
                            return Err(format!(
                                "expected document rejection, but found {count} pages"
                            )
                            .into());
                        }
                        if iteration == 1 {
                            report.exclusions.push(Excluded {
                                page: None,
                                reason: rules.reason.clone().unwrap_or_default(),
                            });
                        }
                        return Ok(());
                    }
                    if count <= 0
                        || case
                            .page_count
                            .or(rules.page_count)
                            .is_some_and(|n| n != count as usize)
                        || report.page_count.is_some_and(|n| n != count as usize)
                    {
                        return Err(format!("unexpected page count: {count}").into());
                    }
                    report.page_count = Some(count as usize);
                    for index in selected_pages(case, count as usize)? {
                        let start = Instant::now();
                        let page = bindings.FPDF_LoadPage(document, index as i32);
                        let load_ms = start.elapsed().as_secs_f64() * 1_000.0;
                        if page.is_null() {
                            if let Some(reason) = rules.rejected_pages.get(&index) {
                                if iteration == 1 {
                                    report.exclusions.push(Excluded {
                                        page: Some(index),
                                        reason: reason.clone(),
                                    });
                                }
                                continue;
                            }
                            return Err(format!("page {index}: load failed").into());
                        }
                        if rules.rejected_pages.contains_key(&index) {
                            bindings.FPDF_ClosePage(page);
                            return Err(format!(
                                "page {index}: expected rejection, but page now loads"
                            )
                            .into());
                        }
                        let dimensions = page_dimensions(bindings.as_ref(), page, case.dpi);
                        if let Some(reason) = rules.invalid_geometry.get(&index) {
                            bindings.FPDF_ClosePage(page);
                            if dimensions.is_ok() {
                                return Err(format!(
                                    "page {index}: expected invalid geometry, but page now renders"
                                )
                                .into());
                            }
                            if iteration == 1 {
                                report.exclusions.push(Excluded {
                                    page: Some(index),
                                    reason: reason.clone(),
                                });
                            }
                            continue;
                        }
                        let result = dimensions.and_then(|(width, height)| {
                            render_page(bindings.as_ref(), page, None, case)
                                .map(|timing| (timing, width, height))
                        });
                        bindings.FPDF_ClosePage(page);
                        let (timing, width, height) = result?;
                        if iteration != 0 {
                            report.samples.push(Sample {
                                page: index,
                                iteration,
                                width,
                                height,
                                load_ms,
                                render_ms: timing.render_ms,
                                fill_ms: timing.fill_ms,
                            });
                        }
                    }
                    Ok(())
                })();
                bindings.FPDF_CloseDocument(document);
                result?;
                if iteration != 0 {
                    report.document_load_ms.push(document_ms);
                }
                println!(
                    "{backend}/{}: benchmark round {iteration}/{iterations} complete",
                    case.id
                );
            }
            Ok(())
        })();
        bindings.FPDF_DestroyLibrary();
        result?;
    }
    let path = report_path(backend, case);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

fn median_timings(samples: &[(f64, f64, f64)]) -> Timing {
    Timing {
        fill_ms: median(samples.iter().map(|&(_, _, fill)| fill).collect()),
        load_fill_draw_ms: median(
            samples
                .iter()
                .map(|&(load, render, fill)| load + fill + render)
                .collect(),
        ),
        load_ms: median(samples.iter().map(|&(load, _, _)| load).collect()),
        render_ms: median(samples.iter().map(|&(_, render, _)| render).collect()),
        total_ms: median(
            samples
                .iter()
                .map(|&(load, render, _)| load + render)
                .collect(),
        ),
    }
}

fn document_total_ms(document: &Document) -> Option<f64> {
    if document.samples.is_empty() {
        return None;
    }
    let mut rounds = document.document_load_ms.clone();
    for sample in &document.samples {
        rounds[sample.iteration - 1] += sample.load_ms + sample.fill_ms + sample.render_ms;
    }
    Some(median(rounds))
}

fn validate_pair(case: &Case, a: &Document, b: &Document, iterations: usize) -> Result<()> {
    if a.case != case.id
        || b.case != case.id
        || a.backend == b.backend
        || a.page_count != b.page_count
        || a.exclusions != b.exclusions
    {
        return Err("benchmark backend inventories or exclusions differ".into());
    }
    let inventory = |doc: &Document| -> Result<BTreeMap<_, _>> {
        let mut samples = BTreeMap::new();
        for s in &doc.samples {
            if !(1..=iterations).contains(&s.iteration)
                || [s.load_ms, s.render_ms, s.fill_ms]
                    .iter()
                    .any(|v| !v.is_finite() || *v < 0.)
                || samples
                    .insert((s.page, s.iteration), (s.width, s.height))
                    .is_some()
            {
                return Err("invalid or duplicate benchmark sample".into());
            }
        }
        let mut pages = std::collections::BTreeSet::new();
        for &(page, _) in samples.keys() {
            for iteration in 1..=iterations {
                if !samples.contains_key(&(page, iteration)) {
                    return Err("missing benchmark iteration".into());
                }
            }
            pages.insert(page);
        }
        for excluded in &doc.exclusions {
            if let Some(page) = excluded.page {
                if !pages.insert(page) {
                    return Err("duplicate benchmark page outcome".into());
                }
            }
        }
        if let Some(count) = doc.page_count {
            if doc.document_load_ms.len() != iterations
                || pages != selected_pages(case, count)?.collect()
            {
                return Err("benchmark page inventory differs from selection".into());
            }
        } else if doc.document_load_ms.len() != iterations
            || !samples.is_empty()
            || doc.exclusions.len() != 1
            || doc.exclusions[0].page.is_some()
        {
            return Err("unrecorded benchmark document rejection".into());
        }
        Ok(samples)
    };
    if inventory(a)? != inventory(b)? {
        return Err("benchmark backend samples or dimensions differ".into());
    }
    Ok(())
}

pub(super) fn run(
    cases: &[Case],
    selection: &str,
    iterations: usize,
    timeout: std::time::Duration,
    baseline_path: &Path,
    reverse_order: bool,
    output: Option<&Path>,
) -> Result<()> {
    if cfg!(debug_assertions) || iterations == 0 {
        return Err("bench requires --release and --iterations greater than zero".into());
    }
    for case in cases {
        validate_input(case)?;
    }
    let mut documents = Vec::new();
    let baseline = check::Baseline::read(baseline_path)?;
    let raw_directory = output.map_or_else(directory, |path| {
        path.parent().unwrap_or(Path::new(".")).join("raw")
    });
    fs::create_dir_all(&raw_directory)?;
    let progress = raw_directory.join("progress.json");
    let mut excluded_cases = Vec::new();
    let mut dimensions = BTreeMap::new();
    let mut grouped: BTreeMap<(String, usize), [Vec<(f64, f64, f64)>; 2]> = BTreeMap::new();
    // Run backends serially so they do not compete for CPU or memory bandwidth.
    for (case_index, case) in cases.iter().enumerate() {
        let mut rules = match baseline.cases.get(&case.id) {
            Some(rules) => rules.clone(),
            None if case.id.starts_with("local/") => check::Rules::default(),
            None => return Err(format!("{}: missing from comparison baseline", case.id).into()),
        };
        rules.expect = Some(rules.expect.unwrap_or(case.expectation));
        if rules.expect == Some(Expectation::NoCrash) {
            excluded_cases.push(
                serde_json::json!({"case":case.id,"reason":rules.reason,"expect":"no-crash"}),
            );
            continue;
        }
        let mut order = ["native", "rust"];
        if (case_index % 2 == 1) ^ reverse_order {
            order.reverse();
        }
        let mut paired = Vec::new();
        for backend in order {
            fs::write(
                &progress,
                serde_json::to_vec_pretty(&serde_json::json!({
                    "case":case.id,"case_index":case_index,"cases":cases.len(),
                    "backend":backend,"dpi":case.dpi,"complete":false
                }))?,
            )?;
            let backend_index = usize::from(backend == "rust");
            let mut worker = Command::new(std::env::current_exe()?);
            let path = report_path_at(&raw_directory, backend, case);
            if path.exists() {
                fs::remove_file(&path)?;
            }
            worker.env("PDFOXIUM_BENCH_OUTPUT", &raw_directory);
            worker.args([
                "_bench",
                backend,
                &serde_json::to_string(case)?,
                &iterations.to_string(),
                &serde_json::to_string(&rules)?,
            ]);
            pdfoxium_diff::process::run(&mut worker, timeout)?;
            let report: Document = serde_json::from_slice(&fs::read(path)?)?;
            if report.backend != backend || report.case != case.id {
                return Err("unexpected benchmark worker identity".into());
            }
            for sample in &report.samples {
                let key = (case.id.clone(), sample.page);
                let size = (sample.width, sample.height, case.dpi);
                if dimensions
                    .insert(key, size)
                    .is_some_and(|previous| previous != size)
                {
                    return Err("benchmark backend dimensions differ".into());
                }
                grouped.entry((case.id.clone(), sample.page)).or_default()[backend_index].push((
                    sample.load_ms,
                    sample.render_ms,
                    sample.fill_ms,
                ));
            }
            paired.push(report);
        }
        validate_pair(case, &paired[0], &paired[1], iterations)?;
        documents.extend(paired);
    }
    let mut rows = Vec::new();
    for ((case, page), samples) in grouped {
        let timings = samples.map(|samples| median_timings(&samples));
        let (width, height, dpi) = dimensions[&(case.clone(), page)];
        rows.push(Row {
            dpi,
            width,
            height,
            case,
            page,
            native: timings[0],
            rust: timings[1],
        });
    }
    let native_total: f64 = rows.iter().map(|row| row.native.total_ms).sum();
    let rust_total: f64 = rows.iter().map(|row| row.rust.total_ms).sum();
    let document_total = |backend: &str| {
        documents
            .iter()
            .filter(|d| d.backend == backend)
            .filter_map(document_total_ms)
            .sum::<f64>()
    };
    let native_document_total = document_total("native");
    let rust_document_total = document_total("rust");
    let mut csv = String::from("case,page,native_load_ms,native_render_ms,native_total_ms,rust_load_ms,rust_render_ms,rust_total_ms,rust_over_native,dpi,width,height,native_fill_ms,rust_fill_ms,native_load_fill_draw_ms,rust_load_fill_draw_ms,rust_draw_slowdown_pct,rust_total_slowdown_pct,rust_load_fill_draw_slowdown_pct\n");
    for row in &rows {
        csv.push_str(&format!(
            "{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.3},{},{},{},{:.6},{:.6},{:.6},{:.6},{:.3},{:.3},{:.3}\n",
            row.case,
            row.page,
            row.native.load_ms,
            row.native.render_ms,
            row.native.total_ms,
            row.rust.load_ms,
            row.rust.render_ms,
            row.rust.total_ms,
            row.rust.total_ms / row.native.total_ms,
            row.dpi,
            row.width,
            row.height,
            row.native.fill_ms,
            row.rust.fill_ms,
            row.native.load_fill_draw_ms,
            row.rust.load_fill_draw_ms,
            (row.rust.render_ms / row.native.render_ms - 1.0) * 100.0,
            (row.rust.total_ms / row.native.total_ms - 1.0) * 100.0,
            (row.rust.load_fill_draw_ms / row.native.load_fill_draw_ms - 1.0) * 100.0
        ));
    }
    let results_directory = if let Some(path) = output {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else if cases.len() > 1 {
        directory().join(if cases[0].dpi == 72 {
            selection.replace(['/', '\\'], "-")
        } else {
            format!(
                "{}-dpi-{}",
                selection.replace(['/', '\\'], "-"),
                cases[0].dpi
            )
        })
    } else {
        let case = &cases[0];
        directory().join(case.page.map_or_else(
            || case.run_id(),
            |page| format!("{}-page-{page}", case.run_id()),
        ))
    };
    fs::create_dir_all(&results_directory)?;
    fs::write(results_directory.join("pages.csv"), csv)?;
    let excluded_documents = documents
        .iter()
        .filter(|d| d.exclusions.iter().any(|e| e.page.is_none()))
        .count()
        / 2;
    let excluded_pages = documents
        .iter()
        .flat_map(|d| &d.exclusions)
        .filter(|e| e.page.is_some())
        .count()
        / 2;
    fs::write(
        output.map_or_else(|| results_directory.join("results.json"), Path::to_path_buf),
        serde_json::to_vec_pretty(&serde_json::json!({
            "profile": "release", "dpi": cases[0].dpi, "measured_iterations": iterations,
            "libraries": { "native": library_path("native"), "rust": library_path("rust") },
            "warmup_iterations": 1, "method": "fresh document each round; sequential pages; page load, background fill and draw separately; total_ms remains load+draw; load_fill_draw_ms includes fill; bitmap allocation, PNG encoding, closing and file writes excluded; wall-clock milliseconds",
            "architecture": std::env::consts::ARCH, "os": std::env::consts::OS,
            "baseline":baseline_path,"cases":cases,"excluded_cases":excluded_cases,
            "backend_order":"alternating per case", "reverse_order":reverse_order,
            "exclusions":{"documents":excluded_documents,"pages":excluded_pages},
            "summed_page_medians_ms": { "native": native_total, "rust": rust_total },
            "summed_document_medians_ms": { "native": native_document_total, "rust": rust_document_total },
            "document_total_method":"median of each complete measured round's document open + selected page load/fill/draw, then summed across renderable documents; preserves correlations between stages/pages",
            "pages": rows, "documents": documents
        }))?,
    )?;
    for (label, key) in [
        ("Largest relative Rust drawing slowdown", 4),
        ("Largest extra Rust drawing time", 3),
        ("Largest extra Rust page load + draw", 2),
        ("Slowest Rust drawing", 0),
        ("Slowest Rust page loading/parsing", 1),
    ] {
        rows.sort_by(|a, b| {
            let score = |row: &Row| match key {
                0 => row.rust.render_ms,
                1 => row.rust.load_ms,
                3 => row.rust.render_ms - row.native.render_ms,
                4 => row.rust.render_ms / row.native.render_ms - 1.0,
                _ => row.rust.total_ms - row.native.total_ms,
            };
            score(b).total_cmp(&score(a))
        });
        println!("\n{label} (median milliseconds; zero-based pages)");
        for row in rows.iter().take(10) {
            println!("{}/{}: native load {:.3}, fill {:.3}, draw {:.3}; Rust load {:.3}, fill {:.3}, draw {:.3}; extra draw {:+.3} ({:+.1}%), total {:+.3} ({:+.1}%); total ratio {:.2}x", row.case, row.page,
                row.native.load_ms, row.native.fill_ms, row.native.render_ms, row.rust.load_ms, row.rust.fill_ms, row.rust.render_ms,
                row.rust.render_ms - row.native.render_ms, (row.rust.render_ms / row.native.render_ms - 1.0) * 100.0,
                row.rust.total_ms - row.native.total_ms, (row.rust.total_ms / row.native.total_ms - 1.0) * 100.0,
                row.rust.total_ms / row.native.total_ms);
        }
    }
    if !rows.is_empty() {
        println!("\nCorpus context, summed per-page medians: native {native_total:.3} ms, Rust {rust_total:.3} ms ({:.3}x)", rust_total / native_total);
        println!("Document open + page load/fill/draw, summed document medians: native {native_document_total:.3} ms, Rust {rust_document_total:.3} ms ({:.3}x)", rust_document_total / native_document_total);
    }
    fs::write(
        &progress,
        serde_json::to_vec_pretty(&serde_json::json!({
            "complete":true,"pages":rows.len(),"cases":cases.len(),
            "excluded_pages":excluded_pages,"excluded_documents":excluded_documents,
            "excluded_cases":excluded_cases
        }))?,
    )?;
    println!(
        "\n{} pages benchmarked; results in {}",
        rows.len(),
        results_directory.display()
    );
    println!("Policy exclusions: {excluded_documents} documents and {excluded_pages} pages");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair_document(backend: &str) -> Document {
        Document {
            backend: backend.into(),
            case: "blank".into(),
            page_count: Some(3),
            document_load_ms: vec![0.1; 2],
            exclusions: vec![Excluded {
                page: Some(1),
                reason: "known rejection".into(),
            }],
            samples: [0, 2]
                .into_iter()
                .flat_map(|page| {
                    (1..=2).map(move |iteration| Sample {
                        page,
                        iteration,
                        width: 32,
                        height: 16,
                        load_ms: 1.,
                        render_ms: 2.,
                        fill_ms: 0.1,
                    })
                })
                .collect(),
        }
    }

    #[test]
    fn document_totals_preserve_correlations_between_opening_and_pages() {
        let mut doc = pair_document("rust");
        doc.document_load_ms = vec![0., 100., 0.];
        doc.samples = [0, 2]
            .into_iter()
            .flat_map(|page| {
                (1..=3).map(move |iteration| Sample {
                    page,
                    iteration,
                    width: 32,
                    height: 16,
                    render_ms: 0.,
                    fill_ms: 0.,
                    load_ms: if (page == 0 && iteration == 1) || (page == 2 && iteration == 3) {
                        100.
                    } else {
                        0.
                    },
                })
            })
            .collect();
        // Each complete round costs 100; summing the three separate medians
        // (open, first page, second page) would incorrectly report zero.
        assert_eq!(document_total_ms(&doc), Some(100.));
        doc.samples.clear();
        assert_eq!(document_total_ms(&doc), None);
    }

    #[test]
    fn missing_inputs_and_directories_cannot_masquerade_as_pdf_rejections() {
        let mut case = select_case("blank").unwrap();
        validate_input(&case).unwrap();
        case.path = project_root().join("target");
        assert!(validate_input(&case).is_err());
        case.path = project_root().join(format!(
            "target/missing-benchmark-input-{}.pdf",
            std::process::id()
        ));
        assert!(validate_input(&case).is_err());
    }

    #[test]
    fn paired_samples_cover_every_selected_page_and_iteration() {
        let mut case = select_case("blank").unwrap();
        case.page_count = Some(3);
        let native = pair_document("native");
        let mut rust = pair_document("rust");
        validate_pair(&case, &native, &rust, 2).unwrap();
        rust.samples.pop();
        assert!(validate_pair(&case, &native, &rust, 2).is_err());
        let mut native = pair_document("native");
        let mut rust = pair_document("rust");
        native.samples.retain(|s| s.page != 2);
        rust.samples.retain(|s| s.page != 2);
        assert!(validate_pair(&case, &native, &rust, 2).is_err());
    }

    #[test]
    fn backend_rejections_dimensions_and_duplicate_samples_cannot_be_hidden() {
        let mut case = select_case("blank").unwrap();
        case.page_count = Some(3);
        let native = pair_document("native");
        for mutation in 0..3 {
            let mut rust = pair_document("rust");
            match mutation {
                0 => rust.exclusions.clear(),
                1 => rust.samples[0].width += 1,
                _ => rust.samples[1].iteration = 1,
            }
            assert!(validate_pair(&case, &native, &rust, 2).is_err());
        }
    }

    #[test]
    fn stage_medians_and_api_sums_are_computed_independently() {
        let timing = median_timings(&[(1.0, 1.0, 100.0), (1.0, 100.0, 1.0), (100.0, 1.0, 1.0)]);
        assert_eq!(timing.load_ms, 1.0);
        assert_eq!(timing.render_ms, 1.0);
        assert_eq!(timing.fill_ms, 1.0);
        assert_eq!(timing.total_ms, 101.0);
        assert_eq!(timing.load_fill_draw_ms, 102.0);
    }

    #[test]
    fn median_handles_odd_even_and_reordered_samples() {
        assert_eq!(median(vec![9.0, 1.0, 3.0]), 3.0);
        assert_eq!(median(vec![5.0, 1.0, 3.0, 9.0]), 4.0);
    }
}
