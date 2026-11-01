// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Offline summaries of complete benchmark reports. Never load the PDF catalog,
//! source PDFs, comparison baseline or either engine library.

use super::*;
use std::collections::BTreeSet;

const WORKLOADS: [(&str, Option<&str>); 4] = [
    ("Full selected corpus", None),
    ("Hefferon", Some("hefferon_toc")),
    ("Beyond Popular Science", Some("obp_0526")),
    ("Biology 2e", Some("openstax_biology_2e")),
];

#[derive(Deserialize)]
struct Totals {
    native: f64,
    rust: f64,
}

#[derive(Deserialize)]
struct Exclusions {
    documents: usize,
    pages: usize,
}

#[derive(Deserialize)]
struct Skipped {
    case: String,
}

#[derive(Deserialize)]
struct Report {
    dpi: u32,
    measured_iterations: usize,
    warmup_iterations: usize,
    profile: String,
    architecture: String,
    os: String,
    cases: Vec<Case>,
    documents: Vec<Document>,
    excluded_cases: Vec<Skipped>,
    exclusions: Exclusions,
    summed_document_medians_ms: Totals,
}

#[derive(Default)]
struct Summary {
    pages: usize,
    native: f64,
    rust: f64,
}

struct Loaded {
    path: PathBuf,
    report: Report,
    rows: Vec<Row>,
    summaries: [Summary; 4],
}

impl Loaded {
    fn from_report(path: PathBuf, report: Report) -> Result<Self> {
        if report.dpi == 0 || report.measured_iterations == 0 {
            return Err("DPI and measured iteration count must be positive".into());
        }
        let mut cases = BTreeMap::new();
        for case in &report.cases {
            if case.dpi != report.dpi || cases.insert(case.id.as_str(), case).is_some() {
                return Err("duplicate case or inconsistent case DPI".into());
            }
        }
        let mut documents = BTreeMap::<&str, BTreeMap<&str, &Document>>::new();
        for doc in &report.documents {
            if !cases.contains_key(doc.case.as_str())
                || !matches!(doc.backend.as_str(), "native" | "rust")
                || documents
                    .entry(&doc.case)
                    .or_default()
                    .insert(&doc.backend, doc)
                    .is_some()
                || doc
                    .document_load_ms
                    .iter()
                    .any(|v| !v.is_finite() || *v < 0.)
                || doc.samples.iter().any(|s| s.width <= 0 || s.height <= 0)
            {
                return Err("invalid document identity, dimensions or opening times".into());
            }
        }
        let mut skipped = BTreeSet::new();
        for entry in &report.excluded_cases {
            if !cases.contains_key(entry.case.as_str())
                || documents.contains_key(entry.case.as_str())
                || !skipped.insert(entry.case.as_str())
            {
                return Err("invalid or duplicate skipped case".into());
            }
        }
        if documents.len() + skipped.len() != cases.len() {
            return Err("report is missing selected documents".into());
        }
        let mut rows = Vec::new();
        let mut exclusions = Exclusions {
            documents: 0,
            pages: 0,
        };
        for (id, pair) in &documents {
            let native = pair.get("native").ok_or("missing native document")?;
            let rust = pair.get("rust").ok_or("missing Rust document")?;
            validate_pair(cases[id], native, rust, report.measured_iterations)?;
            for exclusion in &native.exclusions {
                if exclusion.page.is_none() {
                    exclusions.documents += 1;
                } else {
                    exclusions.pages += 1;
                }
            }
            let mut samples = BTreeMap::<usize, [Vec<(f64, f64, f64)>; 2]>::new();
            let mut sizes = BTreeMap::new();
            for (index, doc) in [native, rust].into_iter().enumerate() {
                for s in &doc.samples {
                    if sizes
                        .insert(s.page, (s.width, s.height))
                        .is_some_and(|size| size != (s.width, s.height))
                    {
                        return Err("page dimensions change between benchmark rounds".into());
                    }
                    samples.entry(s.page).or_default()[index].push((
                        s.load_ms,
                        s.render_ms,
                        s.fill_ms,
                    ));
                }
            }
            for (page, samples) in samples {
                let [native, rust] = samples.map(|s| median_timings(&s));
                let (width, height) = sizes[&page];
                rows.push(Row {
                    case: (*id).into(),
                    page,
                    dpi: report.dpi,
                    width,
                    height,
                    native,
                    rust,
                });
            }
        }
        if exclusions.documents != report.exclusions.documents
            || exclusions.pages != report.exclusions.pages
        {
            return Err("reported exclusions differ from document outcomes".into());
        }
        let summaries = std::array::from_fn(|i| {
            let selected = |case: &str| WORKLOADS[i].1.is_none_or(|id| case == id);
            let mut summary = Summary {
                pages: rows.iter().filter(|r| selected(&r.case)).count(),
                ..Summary::default()
            };
            for doc in report.documents.iter().filter(|d| selected(&d.case)) {
                let total = document_total_ms(doc).unwrap_or(0.);
                if doc.backend == "native" {
                    summary.native += total;
                } else {
                    summary.rust += total;
                }
            }
            summary
        });
        let recorded = &report.summed_document_medians_ms;
        for (actual, expected) in [
            (summaries[0].native, recorded.native),
            (summaries[0].rust, recorded.rust),
        ] {
            if !actual.is_finite()
                || !expected.is_finite()
                || (actual - expected).abs() > 1e-6 * actual.abs().max(1.)
            {
                return Err("document totals disagree with recorded summary".into());
            }
        }
        Ok(Self {
            path,
            report,
            rows,
            summaries,
        })
    }

    fn inventory(&self) -> BTreeSet<(&str, usize)> {
        self.rows
            .iter()
            .map(|r| (r.case.as_str(), r.page))
            .collect()
    }
}

fn ratio(numerator: f64, denominator: f64) -> Option<f64> {
    let value = numerator / denominator;
    (denominator > 0. && value.is_finite()).then_some(value)
}

fn factor(numerator: f64, denominator: f64) -> String {
    ratio(numerator, denominator).map_or_else(|| "n/a".into(), |v| format!("{v:.2}×"))
}

fn readme_table(reports: &[Loaded]) -> String {
    let mut out = String::from("| Workload | Timed pages |");
    for r in reports {
        out.push_str(&format!(" Speedup at {} DPI |", r.report.dpi));
    }
    out.push_str("\n|---|---:|");
    for _ in reports {
        out.push_str("---:|");
    }
    out.push('\n');
    for (i, (name, _)) in WORKLOADS.iter().enumerate() {
        let count = reports[0].summaries[i].pages;
        if i != 0 && count == 0 {
            continue;
        }
        // Group thousands exactly as in the README, without a locale dependency.
        let digits = count.to_string();
        let pages: String = digits
            .chars()
            .enumerate()
            .flat_map(|(i, c)| {
                let comma = i > 0 && (digits.len() - i).is_multiple_of(3);
                comma.then_some(',').into_iter().chain(std::iter::once(c))
            })
            .collect();
        out.push_str(&format!("| {name} | {pages} |"));
        for r in reports {
            let s = &r.summaries[i];
            let speedup = factor(s.native, s.rust);
            out.push_str(&if i == 0 {
                format!(" **{speedup}** |")
            } else {
                format!(" {speedup} |")
            });
        }
        out.push('\n');
    }
    out
}

fn ranking<'a>(rows: &'a [Row], kind: usize, min_ms: f64, top: usize) -> Vec<&'a Row> {
    let score = |r: &Row| match kind {
        0 => r.rust.load_fill_draw_ms,
        1 => r.rust.load_fill_draw_ms - r.native.load_fill_draw_ms,
        _ => ratio(r.rust.load_fill_draw_ms, r.native.load_fill_draw_ms).unwrap_or(0.),
    };
    let mut ranked: Vec<_> = rows
        .iter()
        .filter(|r| match kind {
            0 => true,
            1 => score(r) > 0.,
            _ => r.native.load_fill_draw_ms >= min_ms && score(r) > 1.,
        })
        .collect();
    ranked.sort_by(|a, b| {
        score(b)
            .total_cmp(&score(a))
            .then_with(|| a.case.cmp(&b.case))
            .then(a.page.cmp(&b.page))
    });
    ranked.truncate(top);
    ranked
}

fn validate_reports(reports: &[Loaded]) -> Result<()> {
    for pair in reports.windows(2) {
        if pair[0].report.dpi == pair[1].report.dpi {
            return Err("supply one report per DPI; inspect repeated runs separately".into());
        }
        if pair[0].inventory() != pair[1].inventory() {
            return Err(
                "reports have different timed case/page selections; inspect them separately".into(),
            );
        }
    }
    Ok(())
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    let mut paths = Vec::new();
    let mut top = 10;
    let mut min_ms: f64 = 1.;
    let mut readme = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--readme" => readme = true,
            "--top" => top = args.next().ok_or("missing --top count")?.parse()?,
            "--min-ms" => min_ms = args.next().ok_or("missing --min-ms value")?.parse()?,
            _ if arg.starts_with('-') => {
                return Err(format!("unknown bench-report option: {arg}").into())
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    if paths.is_empty() || !min_ms.is_finite() || min_ms < 0. {
        return Err("bench-report needs report paths and a finite, nonnegative --min-ms".into());
    }
    // Validate every file before printing anything. No native library or source
    // input is needed, even when the original paths no longer exist.
    let mut reports = Vec::new();
    for path in paths {
        let load = || -> Result<Loaded> {
            Loaded::from_report(path.clone(), serde_json::from_slice(&fs::read(&path)?)?)
        };
        reports.push(load().map_err(|e| format!("{}: {e}", path.display()))?);
    }
    reports.sort_by_key(|r| r.report.dpi);
    validate_reports(&reports)?;
    print!("{}", readme_table(&reports));
    if readme {
        return Ok(());
    }
    println!("\nSpeedup = PDFium / pdfoxium; higher is faster. Totals include document open + page load/fill/draw, summed complete-round medians.\n");
    println!("| Workload | DPI | PDFium (s) | pdfoxium (s) | Speedup |\n|---|---:|---:|---:|---:|");
    for (i, (name, _)) in WORKLOADS.iter().enumerate() {
        for r in &reports {
            let s = &r.summaries[i];
            if i != 0 && s.pages == 0 {
                continue;
            }
            println!(
                "| {name} | {} | {:.2} | {:.2} | {} |",
                r.report.dpi,
                s.native / 1000.,
                s.rust / 1000.,
                factor(s.native, s.rust)
            );
        }
    }
    for r in &reports {
        let report = &r.report;
        println!("\n{} DPI — {}\n{}/{}, {}; {} measured rounds, {} warmup rounds. Excluded: {} documents, {} pages, {} skipped cases.", report.dpi, r.path.display(), report.os, report.architecture, report.profile, report.measured_iterations, report.warmup_iterations, report.exclusions.documents, report.exclusions.pages, report.excluded_cases.len());
        if top == 0 {
            continue;
        }
        println!("Page rankings use load + fill + draw medians, excluding document opening; page indices are zero-based. Stage columns are PDFium/pdfoxium milliseconds. Rust/native above 1 is slower.");
        for (kind, name) in [
            "Slowest pdfoxium pages",
            "Largest extra page time",
            "Largest relative page slowdown",
        ]
        .iter()
        .enumerate()
        {
            println!(
                "\n{name}{}\n",
                if kind == 2 {
                    format!(" (native >= {min_ms:.3} ms)")
                } else {
                    String::new()
                }
            );
            println!("| Case/page | PDFium ms | pdfoxium ms | Extra ms | Rust/native | Load N/R | Fill N/R | Draw N/R |\n|---|---:|---:|---:|---:|---:|---:|---:|");
            for row in ranking(&r.rows, kind, min_ms, top) {
                let n = &row.native;
                let u = &row.rust;
                let case = row.case.replace('|', "\\|").replace(['\n', '\r'], " ");
                println!("| {case}/{} | {:.3} | {:.3} | {:+.3} | {} | {:.3}/{:.3} | {:.3}/{:.3} | {:.3}/{:.3} |", row.page, n.load_fill_draw_ms, u.load_fill_draw_ms, u.load_fill_draw_ms - n.load_fill_draw_ms, factor(u.load_fill_draw_ms, n.load_fill_draw_ms), n.load_ms, u.load_ms, n.fill_ms, u.fill_ms, n.render_ms, u.render_ms);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Report {
        let case = serde_json::from_value(serde_json::json!({
            "id": "hefferon_toc", "path": "/does-not-exist/book.pdf", "dpi": 144,
        }))
        .unwrap();
        let documents = ["native", "rust"]
            .into_iter()
            .map(|backend| Document {
                backend: backend.into(),
                case: "hefferon_toc".into(),
                page_count: Some(2),
                document_load_ms: vec![0., 100., 0.],
                exclusions: vec![],
                samples: (0..2)
                    .flat_map(|page| {
                        (1..=3).map(move |iteration| Sample {
                            page,
                            iteration,
                            width: 32,
                            height: 16,
                            load_ms: if (page == 0 && iteration == 1)
                                || (page == 1 && iteration == 3)
                            {
                                100.
                            } else {
                                0.
                            },
                            fill_ms: 0.,
                            render_ms: 0.,
                        })
                    })
                    .collect(),
            })
            .collect();
        Report {
            dpi: 144,
            measured_iterations: 3,
            warmup_iterations: 1,
            profile: "release".into(),
            architecture: "test".into(),
            os: "test".into(),
            cases: vec![case],
            documents,
            excluded_cases: vec![],
            exclusions: Exclusions {
                documents: 0,
                pages: 0,
            },
            summed_document_medians_ms: Totals {
                native: 100.,
                rust: 100.,
            },
        }
    }

    #[test]
    fn summaries_preserve_round_correlations_and_do_not_need_source_files() {
        let loaded = Loaded::from_report("saved.json".into(), fixture()).unwrap();
        assert_eq!(loaded.summaries[0].native, 100.);
        assert_eq!(loaded.summaries[1].rust, 100.);
        // Adding page/stage medians would incorrectly give zero instead.
        assert_eq!(
            loaded
                .rows
                .iter()
                .map(|r| r.rust.load_fill_draw_ms)
                .sum::<f64>(),
            0.
        );
        assert_eq!(readme_table(&[loaded]), "| Workload | Timed pages | Speedup at 144 DPI |\n|---|---:|---:|\n| Full selected corpus | 2 | **1.00×** |\n| Hefferon | 2 | 1.00× |\n");
    }

    #[test]
    fn incomplete_duplicate_and_invalid_reports_are_rejected() {
        for mutation in 0..10 {
            let mut report = fixture();
            match mutation {
                0 => {
                    report.documents.pop();
                }
                1 => {
                    report.documents[1].samples.pop();
                }
                2 => report.documents[1].samples[0].width += 1,
                3 => report.documents[1].backend = "native".into(),
                4 => report.documents[1].document_load_ms[0] = f64::NAN,
                5 => report.documents[1].samples[0].render_ms = -1.,
                6 => report.summed_document_medians_ms.rust = 0.,
                7 => report.cases.push(report.cases[0].clone()),
                8 => {
                    for doc in &mut report.documents {
                        doc.samples.retain(|s| s.page == 0);
                    }
                }
                _ => {
                    for doc in &mut report.documents {
                        doc.samples[0].width += 1;
                    }
                }
            }
            assert!(
                Loaded::from_report("bad.json".into(), report).is_err(),
                "mutation {mutation}"
            );
        }
    }

    #[test]
    fn multiple_dpis_require_matching_pages_and_cannot_merge_repeated_runs() {
        let first = Loaded::from_report("144.json".into(), fixture()).unwrap();
        let mut other = fixture();
        other.dpi = 288;
        other.cases[0].dpi = 288;
        let second = Loaded::from_report("288.json".into(), other).unwrap();
        let mut reports = vec![first, second];
        validate_reports(&reports).unwrap();
        assert!(readme_table(&reports).contains("| Hefferon | 2 | 1.00× | 1.00× |"));
        reports[1].report.dpi = 144;
        assert!(validate_reports(&reports).is_err());
        reports[1].report.dpi = 288;
        reports[1].rows[1].page = 7;
        assert!(validate_reports(&reports).is_err());
    }

    #[test]
    fn rankings_distinguish_absolute_cost_relative_slowdown_and_zero_times() {
        let mut loaded = Loaded::from_report("saved.json".into(), fixture()).unwrap();
        let row = &mut loaded.rows[0];
        row.native.load_fill_draw_ms = 10.;
        row.rust.load_fill_draw_ms = 12.;
        let row = &mut loaded.rows[1];
        row.native.load_fill_draw_ms = 0.001;
        row.rust.load_fill_draw_ms = 0.1;
        assert_eq!(ranking(&loaded.rows, 0, 1., 1)[0].page, 0);
        assert_eq!(ranking(&loaded.rows, 1, 1., 1)[0].page, 0);
        assert_eq!(ranking(&loaded.rows, 2, 0., 1)[0].page, 1);
        assert_eq!(ranking(&loaded.rows, 2, 1., 1)[0].page, 0);
        loaded.rows[0].rust.load_fill_draw_ms = 9.;
        assert!(ranking(&loaded.rows, 2, 1., 10).is_empty());
        assert!(ranking(&loaded.rows, 0, 0., 0).is_empty());
        assert_eq!(factor(0., 0.), "n/a");
        assert_eq!(factor(1., 0.), "n/a");
    }

    #[test]
    fn paired_rejections_are_excluded_from_totals() {
        let mut report = fixture();
        for doc in &mut report.documents {
            doc.page_count = None;
            doc.samples.clear();
            doc.document_load_ms.fill(999.);
            doc.exclusions.push(Excluded {
                page: None,
                reason: "known rejection".into(),
            });
        }
        report.exclusions.documents = 1;
        report.summed_document_medians_ms = Totals {
            native: 0.,
            rust: 0.,
        };
        let loaded = Loaded::from_report("saved.json".into(), report).unwrap();
        assert_eq!(loaded.summaries[0].native, 0.);
        assert_eq!(loaded.summaries[0].pages, 0);
    }
}
