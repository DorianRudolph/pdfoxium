// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! In-memory, process-isolated corpus gate using the shared C API raster metric.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{mpsc, Arc, Mutex};

const METRIC: &str = "connected-residual-bgra-v2";
const CHUNK_PAGES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub version: u32,
    pub metric: String,
    pub reference_dpi: u32,
    pub default_max_error: f64,
    pub channel_leeway: u8,
    pub edge_mass_divisor: f32,
    pub cases: BTreeMap<String, Rules>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_error: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<Expectation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pages: BTreeMap<usize, Tolerance>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub invalid_geometry: BTreeMap<usize, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rejected_pages: BTreeMap<usize, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tolerance {
    pub max_error: f64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub max_error_by_dpi: BTreeMap<u32, f64>,
    pub reason: String,
}

impl Baseline {
    pub(super) fn read(path: &Path) -> Result<Self> {
        let baseline: Self = serde_json::from_slice(&fs::read(path)?)?;
        if baseline.version != 1 || baseline.metric != METRIC || baseline.reference_dpi == 0 {
            return Err("unsupported comparison baseline version, metric or reference DPI".into());
        }
        validate_limit(baseline.default_max_error)?;
        if !baseline.edge_mass_divisor.is_finite() || baseline.edge_mass_divisor < 1. {
            return Err("edge_mass_divisor must be finite and at least one".into());
        }
        for (id, rules) in &baseline.cases {
            if rules.page_count == Some(0) {
                return Err(format!("{id}: page_count must be positive").into());
            }
            if let Some(limit) = rules.max_error {
                validate_limit(limit)?;
                require_reason(rules.reason.as_deref(), id)?;
            }
            if rules.expect.is_some() {
                require_reason(rules.reason.as_deref(), id)?;
            }
            for tolerance in rules.pages.values() {
                validate_limit(tolerance.max_error)?;
                require_reason(Some(&tolerance.reason), id)?;
                for (&dpi, &limit) in &tolerance.max_error_by_dpi {
                    if dpi == 0 {
                        return Err(format!("{id}: override DPI must be positive").into());
                    }
                    validate_limit(limit)?;
                }
            }
            for reason in rules.invalid_geometry.values() {
                require_reason(Some(reason), id)?;
            }
            for (&page, reason) in &rules.rejected_pages {
                require_reason(Some(reason), id)?;
                if rules.invalid_geometry.contains_key(&page) {
                    return Err(format!("{id}: page {page} has conflicting outcomes").into());
                }
            }
        }
        Ok(baseline)
    }
}

fn validate_limit(limit: f64) -> Result<()> {
    if !limit.is_finite() || limit < 0. {
        return Err("comparison limits must be finite and nonnegative".into());
    }
    Ok(())
}

fn require_reason(reason: Option<&str>, id: &str) -> Result<()> {
    if reason.is_none_or(|s| s.trim().is_empty()) {
        return Err(format!("{id}: every comparison exception needs a reason").into());
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
struct Job {
    case: Case,
    rules: Rules,
    default_max_error: f64,
    channel_leeway: u8,
    edge_mass_divisor: f32,
    expect: Expectation,
    flip: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct PageResult {
    page: usize,
    status: String,
    native: String,
    rust: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f64>,
    max_error: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed_pixels: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maximum_channel_delta: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mean_channel_delta: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comparison_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    residual: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifacts: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    flip: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct CaseResult {
    case: String,
    input: PathBuf,
    dpi: u32,
    expectation: Expectation,
    flags: i32,
    background_bgra: String,
    status: String,
    rules: Rules,
    #[serde(skip_serializing_if = "Option::is_none")]
    native_page_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rust_page_count: Option<usize>,
    pages: Vec<PageResult>,
    errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active: Option<(usize, String)>,
}

impl CaseResult {
    fn new(case: &Case) -> Self {
        Self {
            case: case.id.clone(),
            input: case.input_path(),
            dpi: case.dpi,
            expectation: case.expectation,
            flags: portable_render_flags(case),
            background_bgra: case
                .background_bgra
                .clone()
                .unwrap_or_else(|| "ffffffff".into()),
            status: "pending".into(),
            rules: Rules::default(),
            native_page_count: None,
            rust_page_count: None,
            pages: Vec::new(),
            errors: Vec::new(),
            active: None,
        }
    }
    fn finish(&mut self) {
        self.active = None;
        self.pages.sort_by_key(|p| p.page);
        self.status = if !self.errors.is_empty() || self.pages.iter().any(|p| p.status == "failed")
        {
            "failed"
        } else {
            "passed"
        }
        .into();
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

pub fn run(
    cases: &[Case],
    baseline_path: &Path,
    report_path: &Path,
    jobs: usize,
    timeout: Duration,
    flip: bool,
    expectation: Option<Expectation>,
) -> Result<()> {
    if jobs == 0 {
        return Err("--jobs must be positive".into());
    }
    let baseline = Baseline::read(baseline_path)?;
    let directory = report_path.with_extension("work");
    let artifacts = report_path.with_extension("failures");
    if directory.exists() {
        fs::remove_dir_all(&directory)?;
    }
    fs::create_dir_all(&directory)?;
    if artifacts.exists() {
        fs::remove_dir_all(&artifacts)?;
    }
    let mut tasks = VecDeque::new();
    let mut results: BTreeMap<_, _> = cases
        .iter()
        .map(|case| (case.id.clone(), CaseResult::new(case)))
        .collect();
    let mut remaining = BTreeMap::<String, usize>::new();
    for case in cases {
        let rules = match baseline.cases.get(&case.id) {
            Some(rules) => rules.clone(),
            None if case.id.starts_with("local/") => Rules::default(),
            None => return Err(format!("{}: missing from comparison baseline", case.id).into()),
        };
        let expected = expectation.or(rules.expect).unwrap_or(case.expectation);
        results.get_mut(&case.id).unwrap().rules = rules.clone();
        results.get_mut(&case.id).unwrap().expectation = expected;
        let parts = match split(case, &rules, expected) {
            Ok(parts) => parts,
            Err(error) => {
                let result = results.get_mut(&case.id).unwrap();
                result.errors.push(error.to_string());
                result.finish();
                continue;
            }
        };
        remaining.insert(case.id.clone(), parts.len());
        for part in parts {
            tasks.push_back(Job {
                case: part,
                rules: rules.clone(),
                default_max_error: baseline.default_max_error,
                channel_leeway: baseline.channel_leeway,
                edge_mass_divisor: baseline.edge_mass_divisor,
                expect: expected,
                flip,
            });
        }
    }
    let started = Instant::now();
    write_report(report_path, &baseline, &results, started.elapsed())?;
    let count = tasks.len();
    let tasks = Arc::new(Mutex::new(
        tasks.into_iter().enumerate().collect::<VecDeque<_>>(),
    ));
    let (tx, rx) = mpsc::channel();
    let mut threads = Vec::new();
    for _ in 0..jobs.min(count) {
        let (tasks, tx, directory, artifacts) = (
            tasks.clone(),
            tx.clone(),
            directory.clone(),
            artifacts.clone(),
        );
        threads.push(std::thread::spawn(move || {
            loop {
                let task = tasks.lock().unwrap().pop_front();
                let Some((index, job)) = task else { break };
                let path = directory.join(format!("{index:06}.json"));
                let log_path = directory.join(format!("{index:06}.log"));
                let mut result = CaseResult::new(&job.case);
                let executed = (|| -> Result<()> {
                    // A fresh process isolates both engines, their document
                    // caches and any abort. No pixel data crosses a disk file.
                    let log = fs::File::create(&log_path)?;
                    let mut command = Command::new(std::env::current_exe()?);
                    command
                        .args([
                            "_check",
                            &serde_json::to_string(&job)?,
                            path.to_str().ok_or("non-UTF8 report path")?,
                            artifacts.to_str().ok_or("non-UTF8 artifact path")?,
                        ])
                        .stdout(log.try_clone()?)
                        .stderr(log);
                    pdfoxium_diff::process::run(&mut command, timeout)
                })();
                match fs::read(&path) {
                    Ok(bytes) => match serde_json::from_slice(&bytes) {
                        Ok(saved) => result = saved,
                        Err(error) => result
                            .errors
                            .push(format!("invalid worker report: {error}")),
                    },
                    Err(error) => result
                        .errors
                        .push(format!("missing worker report: {error}")),
                }
                if let Err(error) = executed {
                    result
                        .errors
                        .push(format!("{error}; worker log: {}", log_path.display()));
                }
                result.finish();
                if tx.send(result).is_err() {
                    break;
                }
            }
        }));
    }
    drop(tx);
    let mut checkpoint = Instant::now();
    for result in rx {
        let id = result.case.clone();
        let aggregate = results.get_mut(&id).unwrap();
        aggregate.native_page_count = result.native_page_count.or(aggregate.native_page_count);
        aggregate.rust_page_count = result.rust_page_count.or(aggregate.rust_page_count);
        aggregate.pages.extend(result.pages);
        aggregate.errors.extend(result.errors);
        let left = remaining.get_mut(&id).unwrap();
        *left -= 1;
        if *left == 0 {
            aggregate.finish();
            println!(
                "{id}: {} ({} pages, maximum residual {:.6})",
                aggregate.status,
                aggregate.pages.len(),
                aggregate
                    .pages
                    .iter()
                    .filter_map(|p| p.score)
                    .fold(0., f64::max)
            );
        }
        if checkpoint.elapsed() >= Duration::from_secs(1) {
            write_report(report_path, &baseline, &results, started.elapsed())?;
            checkpoint = Instant::now();
        }
    }
    for thread in threads {
        thread
            .join()
            .map_err(|_| "comparison supervisor panicked")?;
    }
    write_report(report_path, &baseline, &results, started.elapsed())?;
    let failed = results.values().filter(|r| r.status == "failed").count();
    println!(
        "Compared {} pages in {} cases: {failed} failed; {}",
        results.values().map(|r| r.pages.len()).sum::<usize>(),
        results.len(),
        report_path.display()
    );
    if failed != 0 {
        return Err(format!("{failed} comparison case(s) failed").into());
    }
    Ok(())
}

fn split(case: &Case, rules: &Rules, expected: Expectation) -> Result<Vec<Case>> {
    let Some(count) = case
        .page_count
        .or(rules.page_count)
        .filter(|_| expected == Expectation::Render)
    else {
        return Ok(vec![case.clone()]);
    };
    let pages = case.selected_pages(count)?;
    Ok((pages.start..pages.end)
        .step_by(CHUNK_PAGES)
        .map(|first| {
            let mut part = case.clone();
            part.page = None;
            part.all_pages = false;
            part.first_page = Some(first);
            part.last_page = Some((first + CHUNK_PAGES).min(pages.end) - 1);
            part
        })
        .collect())
}

fn write_report(
    path: &Path,
    baseline: &Baseline,
    cases: &BTreeMap<String, CaseResult>,
    elapsed: Duration,
) -> Result<()> {
    let gallery = write_gallery(&path.with_extension("failures"), cases)?;
    write_json(
        path,
        &serde_json::json!({
            "version":1, "metric":baseline.metric,
            "reference_dpi":baseline.reference_dpi,
            "default_max_error":baseline.default_max_error,
            "channel_leeway":baseline.channel_leeway,
            "edge_mass_divisor":baseline.edge_mass_divisor,
            "summary":{
                "cases":cases.len(),
                "passed_cases":cases.values().filter(|c| c.status == "passed").count(),
                "failed_cases":cases.values().filter(|c| c.status == "failed").count(),
                "pending_cases":cases.values().filter(|c| c.status == "pending").count(),
                "pages":cases.values().map(|c| c.pages.len()).sum::<usize>(),
                "failed_pages":cases.values().flat_map(|c| &c.pages).filter(|p| p.status == "failed").count(),
                "exception_pages":cases.values().flat_map(|c| &c.pages).filter(|p| p.status == "exception").count()
            },
            "failure_gallery":gallery,
            "method":"one fresh native/Rust document pair per at most 64 selected pages; raw BGRA comparison in memory; PNGs only for failed pages",
            "libraries":{"native":library_path("native"),"rust":library_path("rust")},
            "elapsed_seconds":elapsed.as_secs_f64(),
            "cases":cases
        }),
    )
}

fn html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn write_gallery(root: &Path, cases: &BTreeMap<String, CaseResult>) -> Result<Option<PathBuf>> {
    let mut entries = String::new();
    for case in cases.values() {
        for page in &case.pages {
            let Some(directory) = &page.artifacts else {
                continue;
            };
            let path = directory.join("triple.png");
            let relative = path.strip_prefix(root)?.to_string_lossy();
            let mut url = String::new();
            for c in relative.bytes() {
                if c.is_ascii_alphanumeric() || b"/-_.".contains(&c) {
                    url.push(c as char);
                } else {
                    url.push_str(&format!("%{c:02X}"));
                }
            }
            entries.push_str(&format!("<article><h2>{} — page index {}, {} DPI</h2><p>Residual {} / limit {}. {} {}</p><a href=\"{}\"><img loading=\"lazy\" src=\"{}\" alt=\"PDFium, pdfoxium, raw difference\"></a></article>\n",
                html(&case.case), page.page, case.dpi,
                page.score.map_or_else(|| "not scored".into(), |s| format!("{s:.6}")),
                page.max_error, html(page.reason.as_deref().unwrap_or("")),
                html(page.error.as_deref().unwrap_or("")), url, url));
        }
    }
    if entries.is_empty() {
        return Ok(None);
    }
    let path = root.join("index.html");
    fs::write(&path, format!("<!doctype html><meta charset=\"utf-8\"><title>Rendering differences</title><style>body{{font:15px system-ui;margin:24px;background:#eee;color:#222}}article{{background:white;margin:24px 0;padding:16px}}h2{{font-size:17px}}img{{display:block;max-width:100%;height:auto}}</style><h1>Rendering differences</h1><p>PDFium · pdfoxium · raw difference. Click a triple to view it at full resolution.</p>{entries}"))?;
    Ok(Some(path))
}

// Both libraries can coexist, as in tests/capi/harness.cpp. Every call and
// handle stays with its own binding table; documents drop before their library.
struct Engine {
    api: Box<dyn PdfiumLibraryBindings>,
}
impl Engine {
    fn new(backend: &str) -> Result<Self> {
        let api = Pdfium::bind_to_library(library_path(backend))?;
        // SAFETY: This process owns this binding table's library lifecycle.
        unsafe { api.FPDF_InitLibrary() };
        let engine = Self { api };
        font_extensions::configure_test_limits(backend)?;
        Ok(engine)
    }
    fn open(&self, case: &Case) -> Option<Document<'_>> {
        let path = case.input_path();
        // SAFETY: Path/password are live strings; only this API closes handles.
        let handle = unsafe {
            self.api
                .FPDF_LoadDocument(path.to_str()?, case.password.as_deref())
        };
        if handle.is_null() {
            None
        } else {
            Some(Document {
                engine: self,
                handle,
            })
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY: Documents borrow Engine and therefore have already closed.
        unsafe { self.api.FPDF_DestroyLibrary() };
    }
}
struct Document<'a> {
    engine: &'a Engine,
    handle: pdfium_render::prelude::FPDF_DOCUMENT,
}
impl Document<'_> {
    fn count(&self) -> usize {
        // SAFETY: The document belongs to this live binding table.
        unsafe { self.engine.api.FPDF_GetPageCount(self.handle).max(0) as usize }
    }
    fn paint(&self, page: usize, case: &Case) -> Paint {
        // SAFETY: A selected in-range page is closed on all paths below.
        let handle = unsafe { self.engine.api.FPDF_LoadPage(self.handle, page as i32) };
        if handle.is_null() {
            return Paint::Rejected;
        }
        let result = if page_dimensions(self.engine.api.as_ref(), handle, case.dpi).is_err() {
            Paint::InvalidGeometry
        } else {
            match render_pixels(self.engine.api.as_ref(), handle, case) {
                Ok((width, height, pixels, _)) => Paint::Pixels(Frame {
                    width,
                    height,
                    pixels,
                }),
                Err(error) => Paint::Error(error.to_string()),
            }
        };
        // SAFETY: This is the matching close for the page loaded above.
        unsafe { self.engine.api.FPDF_ClosePage(handle) };
        result
    }
}
impl Drop for Document<'_> {
    fn drop(&mut self) {
        // SAFETY: This binding table created this document.
        unsafe { self.engine.api.FPDF_CloseDocument(self.handle) };
    }
}
struct Frame {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}
enum Paint {
    Pixels(Frame),
    Rejected,
    InvalidGeometry,
    Error(String),
}
impl Paint {
    fn status(&self) -> &'static str {
        match self {
            Self::Pixels(_) => "rendered",
            Self::Rejected => "rejected",
            Self::InvalidGeometry => "invalid-geometry",
            Self::Error(_) => "error",
        }
    }
}

pub fn worker(args: &[String]) -> Result<()> {
    let job: Job = serde_json::from_str(args.first().ok_or("missing check job")?)?;
    let path = Path::new(args.get(1).ok_or("missing worker report path")?);
    let artifacts = Path::new(args.get(2).ok_or("missing artifacts path")?);
    let mut result = CaseResult::new(&job.case);
    result.expectation = job.expect;
    result.rules = job.rules.clone();
    result.status = "running".into();
    write_json(path, &result)?;
    if let Err(error) = compare_document(&job, &mut result, path, artifacts) {
        result.errors.push(error.to_string());
    }
    result.finish();
    write_json(path, &result)
}

fn compare_document(
    job: &Job,
    result: &mut CaseResult,
    progress: &Path,
    artifacts: &Path,
) -> Result<()> {
    let case = &job.case;
    if !case.input_path().is_file() {
        return Err("comparison input is missing or is not a file".into());
    }
    if case.input_path().to_str().is_none() {
        return Err("PDF path is not UTF-8".into());
    }
    let native = Engine::new("native")?;
    let rust = Engine::new("rust")?;
    let a = native.open(case);
    let b = rust.open(case);
    let counts = [
        a.as_ref().map_or(0, Document::count),
        b.as_ref().map_or(0, Document::count),
    ];
    result.native_page_count = Some(counts[0]);
    result.rust_page_count = Some(counts[1]);
    if counts.contains(&0) {
        let accepted = match job.expect {
            Expectation::Reject => counts == [0, 0] && job.rules.rejected_pages.is_empty(),
            Expectation::NoCrash => true,
            Expectation::Render => false,
        };
        if !accepted {
            return Err(format!(
                "document rejected: native pages {}, Rust pages {}",
                counts[0], counts[1]
            )
            .into());
        }
        if job.expect != Expectation::NoCrash || counts == [0, 0] {
            return Ok(());
        }
    }
    if job.expect != Expectation::NoCrash && counts[0] != counts[1] {
        return Err(format!(
            "page counts differ: native {}, Rust {}",
            counts[0], counts[1]
        )
        .into());
    }
    if job.expect != Expectation::NoCrash
        && case
            .page_count
            .or(job.rules.page_count)
            .is_some_and(|expected| expected != counts[0])
    {
        return Err(format!("page count {} differs from the recorded case", counts[0]).into());
    }
    let mut rejected = false;
    let selected = selected_pages(case, counts[0].max(counts[1]))?;
    let expects_selected_rejection = job.rules.rejected_pages.is_empty()
        || job
            .rules
            .rejected_pages
            .keys()
            .any(|page| selected.contains(page));
    for page in selected {
        result.active = Some((page, "native".into()));
        write_json(progress, result)?;
        let native = a
            .as_ref()
            .filter(|_| page < counts[0])
            .map_or(Paint::Rejected, |doc| doc.paint(page, case));
        result.active = Some((page, "rust".into()));
        write_json(progress, result)?;
        let rust = b
            .as_ref()
            .filter(|_| page < counts[1])
            .map_or(Paint::Rejected, |doc| doc.paint(page, case));
        result.active = Some((page, "comparison".into()));
        write_json(progress, result)?;
        rejected |= matches!(native, Paint::Rejected) && matches!(rust, Paint::Rejected);
        result
            .pages
            .push(compare_page(job, page, native, rust, artifacts)?);
    }
    if job.expect == Expectation::Reject && expects_selected_rejection && !rejected {
        return Err("expected rejection, but every selected page rendered".into());
    }
    Ok(())
}

fn compare_page(
    job: &Job,
    page: usize,
    native: Paint,
    rust: Paint,
    artifacts: &Path,
) -> Result<PageResult> {
    let tolerance = job.rules.pages.get(&page);
    let limit = tolerance
        .map(|t| {
            t.max_error_by_dpi
                .get(&job.case.dpi)
                .copied()
                .unwrap_or(t.max_error)
        })
        .or(job.rules.max_error)
        .unwrap_or(job.default_max_error);
    let reason = tolerance
        .map(|t| t.reason.clone())
        .or(job.rules.reason.clone());
    let mut result = PageResult {
        page,
        status: "failed".into(),
        native: native.status().into(),
        rust: rust.status().into(),
        width: None,
        height: None,
        score: None,
        max_error: limit,
        reason,
        changed_pixels: None,
        maximum_channel_delta: None,
        mean_channel_delta: None,
        comparison_ms: None,
        residual: None,
        artifacts: None,
        flip: None,
        error: None,
    };
    if job.expect == Expectation::NoCrash
        && !matches!(native, Paint::Error(_))
        && !matches!(rust, Paint::Error(_))
    {
        result.status = "no-crash".into();
        return Ok(result);
    }
    match (native, rust) {
        (Paint::Pixels(a), Paint::Pixels(b)) => {
            if job.rules.rejected_pages.contains_key(&page) {
                result.error = Some("expected page rejection, but this page now renders".into());
            } else if job.rules.invalid_geometry.contains_key(&page) {
                result.error = Some("expected invalid geometry, but this page now renders".into());
            } else if a.width != b.width || a.height != b.height {
                result.error = Some(format!(
                    "dimensions differ: native {}x{}, Rust {}x{}",
                    a.width, a.height, b.width, b.height
                ));
            } else {
                result.width = Some(a.width);
                result.height = Some(a.height);
                let (mut changed, mut max, mut sum) = (0, 0u8, 0u64);
                for (a, b) in a.pixels.chunks_exact(4).zip(b.pixels.chunks_exact(4)) {
                    changed += usize::from(a != b);
                    for (&a, &b) in a.iter().zip(b) {
                        let d = a.abs_diff(b);
                        max = max.max(d);
                        sum += u64::from(d);
                    }
                }
                result.changed_pixels = Some(changed);
                result.maximum_channel_delta = Some(max);
                result.mean_channel_delta = Some(sum as f64 / a.pixels.len() as f64);
                let started = Instant::now();
                let analysis = pdfoxium_diff::raster::analyze_bgra_with_options(
                    &a.pixels,
                    &b.pixels,
                    a.width as usize,
                    a.height as usize,
                    job.channel_leeway,
                    job.edge_mass_divisor,
                );
                let score = analysis.strongest_region.map_or(0., |r| r.mass);
                result.residual = Some(serde_json::json!({
                    "affected_pixels": analysis.affected_pixels,
                    "total_mass": analysis.total_mass,
                    "strongest_region": analysis.strongest_region.map(|r| serde_json::json!({
                        "mass":r.mass,"pixels":r.pixels,"peak":r.peak,"bounds":r.bounds
                    }))
                }));
                result.comparison_ms = Some(started.elapsed().as_secs_f64() * 1000.);
                result.score = Some(score);
                result.status = if score <= limit {
                    if score > job.default_max_error {
                        "exception"
                    } else {
                        "passed"
                    }
                } else {
                    "failed"
                }
                .into();
                if job.flip {
                    let rgb = |frame: &Frame| -> Result<_> {
                        let rgba = rgba_image_from_bgra(
                            frame.width as u32,
                            frame.height as u32,
                            frame.pixels.clone(),
                        )
                        .ok_or("invalid frame")?;
                        Ok(pdfoxium_diff::perceptual::rgb_over_white(&rgba)?)
                    };
                    let map = flip_rs::ldr_flip(&rgb(&a)?, &rgb(&b)?, flip_rs::DEFAULT_PPD)?;
                    result.flip = Some(serde_json::to_value(
                        pdfoxium_diff::perceptual::summarize(&map),
                    )?);
                }
            }
            if result.status == "failed" {
                let directory = artifacts
                    .join(&job.case.id)
                    .join(format!("dpi-{}", job.case.dpi))
                    .join(format!("{page:04}"));
                fs::create_dir_all(&directory)?;
                let a = rgba_image_from_bgra(a.width as u32, a.height as u32, a.pixels)
                    .ok_or("invalid native bitmap")?;
                let b = rgba_image_from_bgra(b.width as u32, b.height as u32, b.pixels)
                    .ok_or("invalid Rust bitmap")?;
                a.save(directory.join("native.png"))?;
                b.save(directory.join("rust.png"))?;
                if a.dimensions() == b.dimensions() {
                    difference(&a, &b)
                        .0
                        .save(directory.join("difference.png"))?;
                }
                triple(&a, &b)?.save(directory.join("triple.png"))?;
                result.artifacts = Some(directory.clone());
                write_json(&directory.join("comparison.json"), &result)?;
            }
        }
        (Paint::Rejected, Paint::Rejected)
            if job.rules.rejected_pages.contains_key(&page)
                || (job.expect != Expectation::Render && job.rules.rejected_pages.is_empty()) =>
        {
            result.status = "expected-rejection".into();
            if let Some(reason) = job.rules.rejected_pages.get(&page) {
                result.reason = Some(reason.clone());
            }
        }
        (Paint::Pixels(_), Paint::Rejected) | (Paint::Rejected, Paint::Pixels(_))
            if job.expect == Expectation::NoCrash =>
        {
            result.status = "no-crash".into()
        }
        (Paint::InvalidGeometry, Paint::InvalidGeometry)
            if job.rules.invalid_geometry.contains_key(&page) =>
        {
            result.status = "expected-invalid-geometry".into();
            result.reason = job.rules.invalid_geometry.get(&page).cloned();
        }
        (Paint::Error(error), _) | (_, Paint::Error(error)) => result.error = Some(error),
        _ => {
            result.error = Some(format!(
                "unexpected page outcomes: native {}, Rust {}",
                result.native, result.rust
            ))
        }
    }
    Ok(result)
}

fn triple(a: &RgbaImage, b: &RgbaImage) -> Result<RgbImage> {
    const HEADER: u32 = 24;
    let column = a.width().max(b.width());
    let width = column.checked_mul(3).ok_or("triple width overflow")?;
    let height = a
        .height()
        .max(b.height())
        .checked_add(HEADER)
        .ok_or("triple height overflow")?;
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(3))
        .ok_or("triple allocation overflow")?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(bytes)?;
    pixels.resize(bytes, 255);
    let mut output =
        RgbImage::from_raw(width, height, pixels).ok_or("invalid triple dimensions")?;
    let white = image::Rgba([255; 4]);
    for y in 0..height - HEADER {
        for x in 0..column {
            let a = a.get_pixel_checked(x, y).unwrap_or(&white);
            let b = b.get_pixel_checked(x, y).unwrap_or(&white);
            let rgb = |p: &image::Rgba<u8>| {
                image::Rgb(std::array::from_fn(|c| {
                    ((u32::from(p[c]) * u32::from(p[3]) + 255 * (255 - u32::from(p[3])) + 127)
                        / 255) as u8
                }))
            };
            output.put_pixel(x, y + HEADER, rgb(a));
            output.put_pixel(x + column, y + HEADER, rgb(b));
            output.put_pixel(
                x + 2 * column,
                y + HEADER,
                image::Rgb(std::array::from_fn(|c| {
                    a[c].abs_diff(b[c]).max(a[3].abs_diff(b[3]))
                })),
            );
        }
    }
    // Fixed diagnostic labels avoid a runtime font dependency.
    for (index, text) in ["PDFIUM", "PDFOXIUM", "DIFF"].iter().enumerate() {
        for (letter, ch) in text.chars().enumerate() {
            let rows = match ch {
                'P' => [30, 17, 17, 30, 16, 16, 16],
                'D' => [30, 17, 17, 17, 17, 17, 30],
                'F' => [31, 16, 16, 30, 16, 16, 16],
                'I' => [31, 4, 4, 4, 4, 4, 31],
                'U' => [17, 17, 17, 17, 17, 17, 14],
                'M' => [17, 27, 21, 21, 17, 17, 17],
                'O' => [14, 17, 17, 17, 17, 17, 14],
                'X' => [17, 17, 10, 4, 10, 17, 17],
                _ => [0; 7],
            };
            for (y, row) in rows.into_iter().enumerate() {
                for x in 0..5 {
                    if row & (1 << (4 - x)) != 0 {
                        for yy in 0..2 {
                            for xx in 0..2 {
                                let px =
                                    index as u32 * column + 4 + letter as u32 * 12 + x * 2 + xx;
                                if px < (index as u32 + 1) * column {
                                    output.put_pixel(px, 4 + y as u32 * 2 + yy, image::Rgb([0; 3]));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    fn job() -> Job {
        Job {
            case: select_case("blank").unwrap(),
            rules: Rules::default(),
            default_max_error: 1.,
            channel_leeway: 0,
            edge_mass_divisor: 1.,
            expect: Expectation::Render,
            flip: false,
        }
    }
    fn frame(value: u8) -> Paint {
        let mut pixels = vec![value; 32 * 16 * 4];
        for p in pixels.chunks_exact_mut(4) {
            p[3] = 255;
        }
        Paint::Pixels(Frame {
            width: 32,
            height: 16,
            pixels,
        })
    }
    fn temp(name: &str) -> PathBuf {
        let dir = project_root().join(format!(
            "target/comparison-gate/tests/{name}-{}",
            std::process::id()
        ));
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn failures_write_labelled_triples_and_passes_write_no_images() {
        let dir = temp("artifacts");
        let passed = compare_page(&job(), 0, frame(255), frame(255), &dir).unwrap();
        assert_eq!(passed.status, "passed");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let failed = compare_page(&job(), 0, frame(0), frame(255), &dir).unwrap();
        assert_eq!(failed.status, "failed");
        let artifacts = failed.artifacts.clone().unwrap();
        for name in [
            "native.png",
            "rust.png",
            "difference.png",
            "comparison.json",
            "triple.png",
        ] {
            assert!(artifacts.join(name).is_file());
        }
        let triple = image::open(artifacts.join("triple.png"))
            .unwrap()
            .into_rgb8();
        assert_eq!(triple.dimensions(), (96, 40));
        assert_eq!(triple.get_pixel(0, 24).0, [0; 3]);
        assert_eq!(triple.get_pixel(32, 24).0, [255; 3]);
        assert_eq!(triple.get_pixel(64, 24).0, [255; 3]);
        assert!(triple
            .view(0, 0, 32, 24)
            .pixels()
            .any(|(_, _, p)| p.0 == [0; 3]));
        let mut case = CaseResult::new(&job().case);
        case.case = "<&\"".into();
        case.pages.push(failed);
        let cases = BTreeMap::from([(case.case.clone(), case)]);
        let gallery = write_gallery(&dir, &cases).unwrap().unwrap();
        let gallery = fs::read_to_string(gallery).unwrap();
        assert!(gallery.contains("&lt;&amp;&quot;"));
        assert!(gallery.contains("blank/dpi-72/0000/triple.png"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn exceptions_keep_scores_and_never_waive_geometry_or_operational_errors() {
        let dir = temp("exceptions");
        let mut job = job();
        job.rules.max_error = Some(1000.);
        job.rules.reason = Some("confirmed recovery difference".into());
        let result = compare_page(&job, 0, frame(0), frame(255), &dir).unwrap();
        assert_eq!(result.status, "exception");
        assert!(result.score.unwrap() > 1.);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        job.rules.pages.insert(
            0,
            Tolerance {
                max_error: 1.,
                max_error_by_dpi: BTreeMap::new(),
                reason: "page override".into(),
            },
        );
        assert_eq!(
            compare_page(&job, 0, frame(0), frame(255), &dir)
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            compare_page(
                &job,
                1,
                Paint::Error("allocation failed".into()),
                frame(255),
                &dir
            )
            .unwrap()
            .status,
            "failed"
        );
        assert_eq!(
            compare_page(
                &job,
                1,
                Paint::InvalidGeometry,
                Paint::InvalidGeometry,
                &dir
            )
            .unwrap()
            .status,
            "failed"
        );
        job.rules
            .invalid_geometry
            .insert(1, "nonpositive page box in source".into());
        assert_eq!(
            compare_page(
                &job,
                1,
                Paint::InvalidGeometry,
                Paint::InvalidGeometry,
                &dir
            )
            .unwrap()
            .status,
            "expected-invalid-geometry"
        );
        let narrow = Paint::Pixels(Frame {
            width: 16,
            height: 16,
            pixels: vec![255; 16 * 16 * 4],
        });
        let mismatched = compare_page(&job, 2, frame(255), narrow, &dir).unwrap();
        assert_eq!(mismatched.status, "failed");
        assert!(mismatched.error.unwrap().contains("dimensions differ"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn reviewed_dpi_limits_apply_only_to_the_declared_page_and_resolution() {
        let dir = temp("dpi-overrides");
        let mut job = job();
        job.rules.pages.insert(
            0,
            Tolerance {
                max_error: 1.,
                max_error_by_dpi: BTreeMap::from([(288, 1000.)]),
                reason: "independently reviewed higher-resolution difference".into(),
            },
        );
        for dpi in [144, 288, 576] {
            job.case.dpi = dpi;
            let result = compare_page(&job, 0, frame(0), frame(255), &dir).unwrap();
            assert_eq!(result.max_error, if dpi == 288 { 1000. } else { 1. });
            assert_eq!(
                result.status,
                if dpi == 288 { "exception" } else { "failed" }
            );
        }
        job.case.dpi = 288;
        assert_eq!(
            compare_page(&job, 1, frame(0), frame(255), &dir)
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            compare_page(
                &job,
                0,
                Paint::Error("render failed".into()),
                frame(255),
                &dir
            )
            .unwrap()
            .status,
            "failed"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn baseline_requires_reasons_and_the_entire_catalog() {
        let baseline =
            Baseline::read(&project_root().join("tests/render_corpus/comparison-baseline.json"))
                .unwrap();
        for case in catalog::catalog().unwrap() {
            assert!(baseline.cases.contains_key(&case.id), "{} missing", case.id);
        }
        assert!(validate_limit(-1.).is_err());
        assert!(validate_limit(f64::NAN).is_err());
        assert!(validate_limit(f64::INFINITY).is_err());
        let dir = temp("config");
        let file = dir.join("bad.json");
        for rules in [
            serde_json::json!({"max_error":2}),
            serde_json::json!({"expect":"reject"}),
            serde_json::json!({"pages":{"0":{"max_error":2,"reason":" "}}}),
            serde_json::json!({"pages":{"0":{"max_error":2,"reason":"reviewed","max_error_by_dpi":{"0":3}}}}),
            serde_json::json!({"pages":{"0":{"max_error":2,"reason":"reviewed","max_error_by_dpi":{"288":-1}}}}),
            serde_json::json!({"unknown":1}),
            serde_json::json!({"rejected_pages":{"0":" "}}),
            serde_json::json!({"rejected_pages":{"0":"rejected"},"invalid_geometry":{"0":"invalid"}}),
        ] {
            fs::write(&file, serde_json::to_vec(&serde_json::json!({"version":1,"metric":METRIC,"reference_dpi":144,"default_max_error":1,"channel_leeway":2,"edge_mass_divisor":16,"cases":{"test":rules}})).unwrap()).unwrap();
            assert!(Baseline::read(&file).is_err());
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn declared_rejections_do_not_allow_other_pages_to_disappear_or_start_rendering() {
        let dir = temp("rejected-pages");
        let mut job = job();
        job.expect = Expectation::Reject;
        job.rules
            .rejected_pages
            .insert(0, "known malformed page".into());
        assert_eq!(
            compare_page(&job, 0, Paint::Rejected, Paint::Rejected, &dir)
                .unwrap()
                .status,
            "expected-rejection"
        );
        assert_eq!(
            compare_page(&job, 1, Paint::Rejected, Paint::Rejected, &dir)
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            compare_page(&job, 0, frame(255), frame(255), &dir)
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            compare_page(&job, 0, Paint::Rejected, frame(255), &dir)
                .unwrap()
                .status,
            "failed"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn chunking_preserves_ranges_and_never_splits_expected_rejection() {
        let mut case = select_case("blank").unwrap();
        case.page_count = Some(200);
        case.first_page = Some(3);
        case.last_page = Some(155);
        let rules = Rules::default();
        let parts = split(&case, &rules, Expectation::Render).unwrap();
        assert_eq!(parts.len(), 3);
        let selected: Vec<_> = parts
            .iter()
            .flat_map(|c| c.selected_pages(200).unwrap())
            .collect();
        assert_eq!(selected, (3..156).collect::<Vec<_>>());
        assert_eq!(split(&case, &rules, Expectation::Reject).unwrap().len(), 1);
        case.page = Some(78);
        assert_eq!(
            split(&case, &rules, Expectation::Render).unwrap()[0]
                .selected_pages(200)
                .unwrap(),
            78..79
        );
        case.page = Some(200);
        assert!(split(&case, &rules, Expectation::Render).is_err());
    }

    #[test]
    fn triple_composites_alpha_and_pads_mismatched_geometry() {
        let a = RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 128]));
        let b = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
        let image = triple(&a, &b).unwrap();
        assert_eq!(image.dimensions(), (6, 26));
        assert_eq!(image.get_pixel(0, 24).0, [255, 127, 127]);
        assert_eq!(image.get_pixel(2, 24).0, [255; 3]);
        assert_eq!(image.get_pixel(3, 25).0, [255; 3]);
        assert_eq!(image.get_pixel(4, 24).0, [255, 128, 128]);
    }
}
