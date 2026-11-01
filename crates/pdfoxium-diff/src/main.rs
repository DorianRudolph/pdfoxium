// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

use image::{RgbImage, RgbaImage};
use pdfium_render::prelude::{Pdfium, PdfiumLibraryBindings};
use pdfoxium_diff::FPDF_NO_NATIVETEXT;
use pdfoxium_diff::{
    catalog::{self, select_case, Case, Expectation},
    library_path, project_root, Result,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

mod bench;
mod check;
mod font_extensions;
mod glyphs;
mod memory;

fn output(backend: &str, case: &Case) -> PathBuf {
    let directory = project_root()
        .join("target/render-diff")
        .join(backend)
        .join(case.run_id());
    case.page.map_or_else(
        || directory.clone(),
        |page| directory.join(format!("page-{page}")),
    )
}

fn selected_pages(case: &Case, count: usize) -> Result<std::ops::Range<usize>> {
    case.selected_pages(count)
}

fn png(directory: &Path, page: usize) -> PathBuf {
    directory.join(format!("{page:04}.png"))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(if error.is::<pdfoxium_diff::process::PdfRejected>() {
            pdfoxium_diff::process::REJECTED_EXIT_CODE
        } else {
            1
        });
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|arg| arg == "--help") {
        println!("pdfoxium-diff <list|fetch|native|rust|diff|check|bench|bench-report|glyphs> <inputs...> [options]");
        println!("Selectors: bundled (43 small checked-in fixtures), books (3 downloadable textbooks), all (46 primary catalog cases), smoke/core/real-world, hayro, hayro/local (286 repository-hosted fixtures), hayro/custom|pdfjs|pdfbox|pdfium|corpus, case ID, PDF path, recursive directory, workspace (all catalogued files already present). Multiple selectors form a union.");
        println!("Download large books explicitly with fetch books. The small Hamiltonian and two-page Right to Privacy conversions are bundled fixtures. Ordinary tests and benchmarks never download implicitly.");
        println!("fetch is explicit and reuses existing PDFs; list/check/bench never download. --available excludes missing files. Hayro passwords/page ranges are preserved; --all-pages overrides ranges. Empty runs fail except list.");
        println!("Workers/downloads time out after 300 seconds by default; --timeout adjusts this. check/diff write target/render-diff/report.json; --report PATH chooses another file. --jobs N sets concurrent workers (default 2).");
        println!("--expect render (default), reject, or no-crash: negative/robustness checks accept only declared API rejections, never crashes/timeouts or harness failures; probe runs do not cache PNGs.");
        println!("bench requires --release; compares native/Rust loading, background fill and drawing without PNG encoding.");
        println!("bench uses the baseline's declared rejected pages/invalid geometry, alternates backend order, and accepts --reverse-order and --report PATH.");
        println!("bench-report REPORT.json... [--readme] [--top N] [--min-ms N] summarizes saved benchmarks without rendering. --readme prints only the README table; --min-ms filters relative rankings by native page time (default 1 ms).");
        println!("--page uses a zero-based index and separate output directories.");
        println!("check/diff compare native/Rust BGRA pixels in memory using the shared raster metric. Only failures emit PNGs, including a labelled triple.png.");
        println!("check/diff default to 144 DPI and tests/render_corpus/comparison-baseline.json; --baseline PATH selects another policy. Other commands default to 72 DPI. glyphs audits letter crops using native text boxes.");
        println!("Native PNGs are cached on demand; --refresh replaces them.");
        println!("check/diff --flip adds experimental FLIP diagnostics to the JSON report; the raster metric determines pass/fail.");
        println!("memory <native|rust> <case-id|file.pdf> [--iterations N] [--window N] [--page INDEX] [--dpi N] [--max-dpi N] [--zoom-steps N] [--trim] measures a persistent document; requires a Linux release build. --trim reclaims glibc free pages after teardown only.");
        println!("memory font controls (Rust): --cache-bytes N --scratch-bytes N --trim-fonts; zero disables retention, 18446744073709551615 is unlimited. CSV includes accounted retention and cumulative drawing time.");
        return Ok(());
    }
    if args[0] == "memory" {
        return memory::run(&args[1..]);
    }
    if args[0] == "bench-report" {
        return bench::report::run(&args[1..]);
    }
    if args[0] == "_check" {
        return check::worker(&args[1..]);
    }
    // One library per process avoids shared PDFium global state and lets the
    // parent report a C ABI abort as a failed render.
    if args[0] == "_render" || args[0] == "_probe" {
        let backend = args.get(1).ok_or("missing worker backend")?;
        let selection = args.get(2).ok_or("missing worker input")?;
        let case: Case = serde_json::from_str(selection)?;
        return render(backend, &case, args[0] == "_render");
    }
    if args[0] == "_bench" {
        let backend = args.get(1).ok_or("missing worker backend")?;
        let case: Case = serde_json::from_str(args.get(2).ok_or("missing worker input")?)?;
        let iterations = args.get(3).ok_or("missing iterations")?.parse()?;
        let rules = args
            .get(4)
            .map(|s| serde_json::from_str(s))
            .transpose()?
            .unwrap_or_default();
        return bench::worker(backend, &case, iterations, &rules);
    }
    let command = args[0].as_str();
    if ![
        "native", "rust", "diff", "check", "bench", "glyphs", "list", "fetch",
    ]
    .contains(&command)
    {
        return Err("unknown command; use --help".into());
    }
    let mut selectors = Vec::new();
    let mut refresh = false;
    let mut flip = false;
    let mut available = false;
    let mut all_pages = false;
    let mut expectation = None;
    let mut page = None;
    let mut iterations = 3usize;
    let mut reverse_order = false;
    let mut bench_report = None;
    let mut dpi = if matches!(command, "check" | "diff") {
        144
    } else {
        72
    };
    let mut timeout = Duration::from_secs(300);
    let mut report_path = project_root().join("target/render-diff/report.json");
    let mut baseline = project_root().join("tests/render_corpus/comparison-baseline.json");
    let mut jobs = 2usize;
    let mut options = args.iter().skip(1);
    while let Some(option) = options.next() {
        match option.as_str() {
            "--refresh" => refresh = true,
            "--flip" if matches!(command, "check" | "diff") => flip = true,
            "--available" => available = true,
            "--all-pages" => all_pages = true,
            "--expect" => {
                expectation = Some(
                    match options.next().ok_or("missing expectation")?.as_str() {
                        "render" => Expectation::Render,
                        "reject" => Expectation::Reject,
                        "no-crash" => Expectation::NoCrash,
                        _ => return Err("expected render, reject or no-crash".into()),
                    },
                )
            }
            "--dpi" => dpi = options.next().ok_or("missing DPI")?.parse()?,
            "--page" => page = Some(options.next().ok_or("missing page index")?.parse()?),
            "--timeout" => {
                timeout =
                    Duration::from_secs(options.next().ok_or("missing timeout seconds")?.parse()?)
            }
            "--report" if matches!(command, "check" | "diff") => {
                report_path = options.next().ok_or("missing report path")?.into()
            }
            "--report" if command == "bench" => {
                bench_report = Some(PathBuf::from(options.next().ok_or("missing report path")?));
            }
            "--reverse-order" if command == "bench" => reverse_order = true,
            "--iterations" if command == "bench" => {
                iterations = options.next().ok_or("missing iterations")?.parse()?
            }
            "--baseline" if matches!(command, "check" | "diff" | "bench") => {
                baseline = options.next().ok_or("missing baseline path")?.into();
            }
            "--jobs" if matches!(command, "check" | "diff") => {
                jobs = options.next().ok_or("missing job count")?.parse()?;
            }
            _ if !option.starts_with("--") => selectors.push(option.clone()),
            _ => return Err("unexpected arguments; use --help".into()),
        }
    }
    if dpi == 0 || timeout.is_zero() || (page.is_some() && all_pages) {
        return Err(
            "DPI/timeout must be positive; --page and --all-pages are mutually exclusive".into(),
        );
    }
    let selection = catalog::select(&selectors, available)?;
    println!(
        "Selected {} PDF(s); {} unavailable excluded",
        selection.cases.len(),
        selection.unavailable
    );
    let mut cases = selection.cases;
    for case in &mut cases {
        case.page = page;
        case.dpi = dpi;
        case.all_pages = all_pages;
        if let Some(expectation) = expectation {
            case.expectation = expectation;
        }
    }
    if command == "list" {
        for case in &cases {
            println!(
                "{}\t{}\tpages={}\texpect={:?}\t{}{}",
                case.id,
                if case.input_path().is_file() {
                    "present"
                } else {
                    "missing"
                },
                case.page_description(),
                case.expectation,
                case.input_path().display(),
                if case.upstream_ignored {
                    " [Hayro ignored; included here]"
                } else {
                    ""
                }
            );
            if let Some(note) = &case.notes {
                println!("  {}", note.replace('\n', "; "));
            }
        }
        return Ok(());
    }
    if cases.is_empty() {
        return Err("selection contains no available PDFs".into());
    }
    if command == "fetch" {
        return pdfoxium_diff::fetch::run(&cases, refresh, timeout);
    }
    if !matches!(command, "check" | "diff" | "bench")
        && cases.iter().any(|c| c.expectation != Expectation::Render)
    {
        return Err("reject/no-crash expectations require check".into());
    }
    if command == "bench" {
        if refresh {
            return Err("bench does not use PNG caches; omit --refresh".into());
        }
        return bench::run(
            &cases,
            &selectors.join("+"),
            iterations,
            timeout,
            &baseline,
            reverse_order,
            bench_report.as_deref(),
        );
    }
    if matches!(command, "check" | "diff") {
        return check::run(
            &cases,
            &baseline,
            &report_path,
            jobs,
            timeout,
            flip,
            expectation,
        );
    }
    let mut failed = 0;
    for case in &cases {
        let result = if command == "glyphs" {
            render_worker("native", case, refresh, timeout)
                .and_then(|()| render_worker("rust", case, false, timeout))
                .and_then(|()| glyphs::audit(case))
        } else {
            render_worker(command, case, refresh, timeout)
        };
        if let Err(error) = result {
            eprintln!("{}: {error}", case.id);
            failed += 1;
        }
    }
    if failed != 0 {
        return Err(format!("{failed} document(s) failed").into());
    }
    Ok(())
}

fn page_count(directory: &Path) -> Result<usize> {
    Ok(fs::read_to_string(directory.join("pages.txt"))?
        .trim()
        .parse()?)
}

fn render_worker(backend: &str, case: &Case, refresh: bool, timeout: Duration) -> Result<()> {
    let directory = output(backend, case);
    if backend == "native"
        && !refresh
        && fs::read_to_string(directory.join("flags.txt"))
            .is_ok_and(|flags| flags == portable_render_flags(case).to_string())
        && page_count(&directory).is_ok_and(|count| {
            count > 0
                && case.page_count.is_none_or(|expected| count == expected)
                && selected_pages(case, count)
                    .is_ok_and(|mut pages| pages.all(|page| png(&directory, page).is_file()))
        })
    {
        println!("{}: using cached native PNGs", case.id);
        return Ok(());
    }
    // Remove previous images before starting so a failed render cannot leave
    // old Rust output looking like a successful new run.
    if directory.exists() {
        fs::remove_dir_all(&directory)?;
    }
    fs::create_dir_all(&directory)?;
    let mut worker = Command::new(std::env::current_exe()?);
    worker.args(["_render", backend, &serde_json::to_string(case)?]);
    pdfoxium_diff::process::run(&mut worker, timeout)
}

fn render(backend: &str, case: &Case, save_images: bool) -> Result<()> {
    let input = fs::File::open(case.input_path())?;
    if !input.metadata()?.is_file() {
        return Err("input is not a PDF file".into());
    }
    let bindings = Pdfium::bind_to_library(library_path(backend))?;
    // SAFETY: The worker initializes one dynamically bound library. All handles
    // and external pixel buffers are kept alive until their matching close;
    // the library is destroyed after the document and its pages are closed.
    unsafe {
        bindings.FPDF_InitLibrary();
        let result = font_extensions::configure_test_limits(backend)
            .and_then(|()| render_document(bindings.as_ref(), backend, case, save_images));
        bindings.FPDF_DestroyLibrary();
        result
    }
}

fn render_document(
    bindings: &dyn PdfiumLibraryBindings,
    backend: &str,
    case: &Case,
    save_images: bool,
) -> Result<()> {
    let path = project_root().join(&case.path);
    let path = path.to_str().ok_or("PDF path is not UTF-8")?;
    // SAFETY: Path/password strings are valid for loading. The document remains
    // live throughout the page loop and is closed on both success and error.
    unsafe {
        let document = bindings.FPDF_LoadDocument(path, case.password.as_deref());
        if document.is_null() {
            return Err(pdfoxium_diff::process::PdfRejected(format!(
                "load failed, PDFium error {}",
                bindings.FPDF_GetLastError()
            ))
            .into());
        }
        let result = (|| {
            let count = bindings.FPDF_GetPageCount(document);
            if count <= 0 {
                return Err(pdfoxium_diff::process::PdfRejected(format!(
                    "invalid page count: {count}"
                ))
                .into());
            }
            if let Some(expected) = case.page_count {
                if count as usize != expected {
                    return Err(format!("expected {expected} pages, got {count}").into());
                }
            }
            let pages = selected_pages(case, count as usize)?;
            let rendered = pages.len();
            for index in pages {
                println!("{}/{index}: rendering {backend}", case.id);
                let page = bindings.FPDF_LoadPage(document, index as i32);
                if page.is_null() {
                    return Err(pdfoxium_diff::process::PdfRejected(format!(
                        "page {index}: load failed"
                    ))
                    .into());
                }
                let image_path = save_images.then(|| png(&output(backend, case), index));
                let result = render_page(bindings, page, image_path.as_deref(), case);
                bindings.FPDF_ClosePage(page);
                result?;
                println!("{}/{index}: rendered {backend}", case.id);
            }
            if save_images {
                fs::write(
                    output(backend, case).join("flags.txt"),
                    portable_render_flags(case).to_string(),
                )?;
                fs::write(output(backend, case).join("pages.txt"), count.to_string())?;
            }
            println!("{}: rendered {rendered} {backend} page(s)", case.id);
            Ok(())
        })();
        bindings.FPDF_CloseDocument(document);
        result
    }
}

struct RenderTiming {
    fill_ms: f64,
    render_ms: f64,
}

fn rgba_image_from_bgra(width: u32, height: u32, mut pixels: Vec<u8>) -> Option<RgbaImage> {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    RgbaImage::from_raw(width, height, pixels)
}

fn portable_render_flags(case: &Case) -> i32 {
    // Use PDFium's portable font renderer on every host. pdfoxium already
    // renders all glyphs through AGG, so this flag has no effect there.
    case.flags | FPDF_NO_NATIVETEXT
}

fn render_page(
    bindings: &dyn PdfiumLibraryBindings,
    page: pdfium_render::prelude::FPDF_PAGE,
    path: Option<&Path>,
    case: &Case,
) -> Result<RenderTiming> {
    let (width, height, pixels, timing) = render_pixels(bindings, page, case)?;
    if let Some(path) = path {
        rgba_image_from_bgra(width as u32, height as u32, pixels)
            .ok_or("invalid pixel buffer")?
            .save(path)?;
    }
    Ok(timing)
}

fn render_pixels(
    bindings: &dyn PdfiumLibraryBindings,
    page: pdfium_render::prelude::FPDF_PAGE,
    case: &Case,
) -> Result<(i32, i32, Vec<u8>, RenderTiming)> {
    // SAFETY: The caller retains the live page. The BGRA Vec supplies exactly
    // height * stride writable bytes and is not resized while the bitmap uses
    // it. Destroying the bitmap releases its borrow before conversion/saving.
    unsafe {
        let (width, height) = page_dimensions(bindings, page, case.dpi)?;
        let stride = width.checked_mul(4).ok_or("bitmap stride overflow")?;
        let length = (stride as usize)
            .checked_mul(height as usize)
            .ok_or("bitmap size overflow")?;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(length)?;
        pixels.resize(length, 255u8);
        let bgra = u32::from_str_radix(case.background_bgra.as_deref().unwrap_or("ffffffff"), 16)?
            .to_be_bytes();
        let bitmap =
            bindings.FPDFBitmap_CreateEx(width, height, 4, pixels.as_mut_ptr().cast(), stride);
        if bitmap.is_null() {
            return Err("bitmap allocation failed".into());
        }
        let fill_start = Instant::now();
        bindings.FPDFBitmap_FillRect(bitmap, 0, 0, width, height, u32::from_le_bytes(bgra) as _);
        let fill_ms = fill_start.elapsed().as_secs_f64() * 1_000.0;
        let start = Instant::now();
        bindings.FPDF_RenderPageBitmap(
            bitmap,
            page,
            0,
            0,
            width,
            height,
            0,
            portable_render_flags(case),
        );
        let render_ms = start.elapsed().as_secs_f64() * 1_000.0;
        bindings.FPDFBitmap_Destroy(bitmap);
        Ok((width, height, pixels, RenderTiming { fill_ms, render_ms }))
    }
}

#[cfg(test)]
mod pixel_diff_util_unittest;
#[cfg(test)]
mod png_codec_tests;

fn scaled_dimension(points: f32, dpi: u32) -> Result<i32> {
    let pixels = (f64::from(points) * f64::from(dpi) / 72.0).ceil();
    if dpi == 0 || !pixels.is_finite() || pixels <= 0.0 || pixels > f64::from(i32::MAX) {
        return Err("invalid page dimensions or DPI".into());
    }
    Ok(pixels as i32)
}

fn page_dimensions(
    bindings: &dyn PdfiumLibraryBindings,
    page: pdfium_render::prelude::FPDF_PAGE,
    dpi: u32,
) -> Result<(i32, i32)> {
    // SAFETY: Called with a live page owned by the render/benchmark worker.
    unsafe {
        Ok((
            scaled_dimension(bindings.FPDF_GetPageWidthF(page), dpi)?,
            scaled_dimension(bindings.FPDF_GetPageHeightF(page), dpi)?,
        ))
    }
}

fn difference(native: &RgbaImage, rust: &RgbaImage) -> (RgbImage, usize, u64) {
    let mut diff = RgbImage::new(native.width(), native.height());
    let mut changed = 0usize;
    let mut total = 0u64;
    for ((a, b), out) in native.pixels().zip(rust.pixels()).zip(diff.pixels_mut()) {
        let delta: [u8; 4] = std::array::from_fn(|i| a[i].abs_diff(b[i]));
        changed += usize::from(delta != [0; 4]);
        total += delta.iter().map(|&v| u64::from(v)).sum::<u64>();
        // Show alpha differences too; the output diff itself is opaque.
        *out = image::Rgb(std::array::from_fn(|i| delta[i].max(delta[3])));
    }
    (diff, changed, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_and_alpha_changes_are_counted_and_visible() {
        let native = RgbaImage::from_pixel(3, 1, image::Rgba([0, 0, 0, 255]));
        let mut rust = native.clone();
        rust.put_pixel(1, 0, image::Rgba([255, 0, 0, 255]));
        rust.put_pixel(2, 0, image::Rgba([0, 0, 0, 0]));
        let (diff, changed, total) = difference(&native, &rust);
        assert_eq!((changed, total), (2, 510));
        assert_eq!(diff.as_raw(), &[0, 0, 0, 255, 0, 0, 255, 255, 255]);
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;
    #[test]
    fn dpi_scales_page_dimensions_and_rejects_invalid_sizes() {
        assert_eq!(scaled_dimension(540.0, 72).unwrap(), 540);
        assert_eq!(scaled_dimension(540.0, 144).unwrap(), 1080);
        assert_eq!(scaled_dimension(540.0, 300).unwrap(), 2250);
        assert_eq!(scaled_dimension(10.25, 72).unwrap(), 11);
        for points in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            assert!(scaled_dimension(points, 300).is_err());
        }
        assert!(scaled_dimension(540.0, 0).is_err());
    }
    #[test]
    fn resolution_and_page_caches_are_separate() {
        let mut case = select_case("blank").unwrap();
        let original = output("native", &case);
        case.dpi = 300;
        assert_ne!(output("native", &case), original);
        case.page = Some(0);
        assert!(output("native", &case).ends_with("blank-dpi-300/page-0"));
    }
}
