// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Render PDF pages to PNG with the safe Rust API.
//!
//! cargo run -p pdfoxium-cli --release -- render input.pdf --output rendered

use pdfoxium::thread::{CancelGroup, PoolOptions, Priority, RenderPool, Source};
use pdfoxium::{Document, RenderConfig, Rotation};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{collections::BTreeSet, ffi::OsString, path::PathBuf, sync::Arc};

type CliResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const HELP: &str = "Render PDF pages to PNG.

Usage: render INPUT.pdf [OPTIONS]

Options:
  -o, --output DIR      Output directory (default: rendered)
  -j, --jobs N          Concurrent page jobs (default: 1)
      --dpi NUMBER     Resolution in dots per inch (default: 144)
      --pages LIST     One-based pages/ranges, e.g. 1-3,7 (default: all)
      --password TEXT  Password for an encrypted PDF
      --rotation DEG   Additional clockwise rotation: 0, 90, 180, 270
      --grayscale      Render in grayscale
      --no-annotations Omit annotations
  -h, --help           Show this help

Selected pages are dispatched in ascending order, once each. Parallel workers
finish in any order. Output files are
named page-0001.png, page-0002.png, etc. Existing PNGs at those paths are replaced.
Use -- before an input filename beginning with a dash.
";

#[derive(Debug)]
struct Options {
    input: PathBuf,
    output: PathBuf,
    dpi: f32,
    jobs: usize,
    pages: Option<Vec<(usize, usize)>>,
    password: Option<String>,
    rotation: Rotation,
    grayscale: bool,
    annotations: bool,
}

fn string_value(value: OsString, flag: &str) -> Result<String, String> {
    value
        .into_string()
        .map_err(|_| format!("{flag} requires a UTF-8 value"))
}

fn page_number(value: &str) -> Result<usize, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("invalid page number {value:?}; pages start at 1"));
    }
    let number = value
        .parse::<usize>()
        .map_err(|_| format!("page number {value:?} is too large"))?;
    if number == 0 {
        return Err("page numbers start at 1".into());
    }
    Ok(number)
}

fn parse_pages(value: &str) -> Result<Vec<(usize, usize)>, String> {
    value
        .split(',')
        .map(|part| {
            let part = part.trim();
            let (start, end) = match part.split_once('-') {
                Some((start, end)) => (page_number(start.trim())?, page_number(end.trim())?),
                None => {
                    let number = page_number(part)?;
                    (number, number)
                }
            };
            if start > end {
                return Err(format!("page range {part:?} is reversed"));
            }
            Ok((start, end))
        })
        .collect()
}

fn selected_pages(ranges: Option<&[(usize, usize)]>, count: usize) -> Result<Vec<usize>, String> {
    let Some(ranges) = ranges else {
        return Ok((0..count).collect());
    };
    // Validate every range before expanding it or creating any output files.
    for &(start, end) in ranges {
        if start == 0 || end < start || end > count {
            return Err(format!(
                "page range {start}-{end} is outside this document's {count} pages"
            ));
        }
    }
    let mut pages = BTreeSet::new();
    for &(start, end) in ranges {
        pages.extend((start - 1)..end);
    }
    Ok(pages.into_iter().collect())
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Option<Options>, String> {
    let mut args = args.into_iter();
    let mut input = None;
    let mut output = None;
    let mut dpi = None;
    let mut jobs = None;
    let mut pages = None;
    let mut password = None;
    let mut rotation = None;
    let mut grayscale = false;
    let mut annotations = true;
    let mut positional_only = false;
    while let Some(arg) = args.next() {
        if !positional_only {
            let flag = arg.to_str().unwrap_or("");
            let mut next = || {
                args.next()
                    .ok_or_else(|| format!("missing value for {flag}"))
            };
            match flag {
                "-h" | "--help" => return Ok(None),
                "--" => {
                    positional_only = true;
                    continue;
                }
                "-o" | "--output" => {
                    if output.is_some() {
                        return Err("--output was specified more than once".into());
                    }
                    let path = PathBuf::from(next()?);
                    if path.as_os_str().is_empty() {
                        return Err("--output requires a nonempty directory".into());
                    }
                    output = Some(path);
                    continue;
                }
                "-j" | "--jobs" => {
                    if jobs.is_some() {
                        return Err("--jobs was specified more than once".into());
                    }
                    let value = string_value(next()?, flag)?;
                    jobs = Some(
                        page_number(&value)
                            .map_err(|_| "--jobs requires a positive integer".to_string())?,
                    );
                    continue;
                }
                "--dpi" => {
                    if dpi.is_some() {
                        return Err("--dpi was specified more than once".into());
                    }
                    let value = string_value(next()?, flag)?;
                    let value = value
                        .parse::<f32>()
                        .map_err(|_| "--dpi requires a positive finite number".to_string())?;
                    if !value.is_finite() || value <= 0.0 {
                        return Err("--dpi requires a positive finite number".into());
                    }
                    dpi = Some(value);
                    continue;
                }
                "--pages" => {
                    if pages.is_some() {
                        return Err("--pages was specified more than once".into());
                    }
                    pages = Some(parse_pages(&string_value(next()?, flag)?)?);
                    continue;
                }
                "--password" => {
                    if password.is_some() {
                        return Err("--password was specified more than once".into());
                    }
                    password = Some(string_value(next()?, flag)?);
                    continue;
                }
                "--rotation" => {
                    if rotation.is_some() {
                        return Err("--rotation was specified more than once".into());
                    }
                    rotation = Some(match string_value(next()?, flag)?.as_str() {
                        "0" => Rotation::None,
                        "90" => Rotation::Cw90,
                        "180" => Rotation::Cw180,
                        "270" => Rotation::Cw270,
                        _ => return Err("--rotation must be 0, 90, 180 or 270".into()),
                    });
                    continue;
                }
                "--grayscale" => {
                    grayscale = true;
                    continue;
                }
                "--no-annotations" => {
                    annotations = false;
                    continue;
                }
                _ if flag.starts_with('-') => return Err(format!("unknown option {flag:?}")),
                _ => {}
            }
        }
        if arg.is_empty() {
            return Err("input filename must not be empty".into());
        }
        if input.replace(PathBuf::from(arg)).is_some() {
            return Err("expected exactly one input PDF".into());
        }
    }
    Ok(Some(Options {
        input: input.ok_or_else(|| "missing input PDF; use --help for usage".to_string())?,
        output: output.unwrap_or_else(|| PathBuf::from("rendered")),
        dpi: dpi.unwrap_or(144.0),
        jobs: jobs.unwrap_or(1),
        pages,
        password,
        rotation: rotation.unwrap_or(Rotation::None),
        grayscale,
        annotations,
    }))
}

fn run(options: Options) -> CliResult<()> {
    // Workers share only immutable source bytes; each parses its own document.
    // Keep lazy file reads for the default single-threaded path.
    let bytes: Option<Arc<[u8]>> = if options.jobs > 1 {
        Some(std::fs::read(&options.input)?.into())
    } else {
        None
    };
    let password = options.password.as_deref().unwrap_or("");
    let document = match &bytes {
        Some(bytes) => Document::from_bytes_with_password(bytes.clone(), password)?,
        None => Document::open_with_password(&options.input, password)?,
    };
    let pages = selected_pages(options.pages.as_deref(), document.page_count()?)?;
    let config = RenderConfig::dpi(options.dpi)
        .rotation(options.rotation)
        .grayscale(options.grayscale)
        .annotations(options.annotations);

    let input = std::fs::canonicalize(&options.input)?;
    for &index in &pages {
        let path = options.output.join(format!("page-{:04}.png", index + 1));
        match std::fs::canonicalize(&path) {
            Ok(existing) if existing == input => {
                return Err(
                    format!("output {} would replace the input PDF", path.display()).into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    std::fs::create_dir_all(&options.output)?;
    if let Some(bytes) = bytes {
        // Validation used this copy; workers create and drop their own copies
        // entirely within their threads. No Document or Page crosses threads.
        drop(document);
        render_parallel(bytes, &pages, &options, &config)?;
    } else {
        for &index in &pages {
            render_page(&document, index, &options, &config)?;
        }
    }
    Ok(())
}

fn render_page(
    document: &Document,
    index: usize,
    options: &Options,
    config: &RenderConfig,
) -> CliResult<()> {
    let page = document.page(index)?;
    let bitmap = page.render(config)?;
    let path = options.output.join(format!("page-{:04}.png", index + 1));
    bitmap.write_png(&path)?;
    // Each worker retains only its current page's parsed content/image cache.
    page.unload()?;
    println!("{}", path.display());
    Ok(())
}

fn render_parallel(
    bytes: Arc<[u8]>,
    pages: &[usize],
    options: &Options,
    config: &RenderConfig,
) -> CliResult<()> {
    if pages.is_empty() {
        return Ok(());
    }
    let jobs = options.jobs.min(pages.len());
    let pool = RenderPool::with_options(
        Source::Bytes(bytes),
        pdfoxium::Config::default(),
        PoolOptions {
            threads: jobs,
            password: options.password.clone(),
            max_pending_jobs: jobs,
            max_cached_pages: 1,
            max_paused_jobs: 0,
            ..PoolOptions::default()
        },
    )?;
    // Each page pipeline waits for a pool render, then encodes its PNG. This
    // preserves parallel encoding while keeping at most `jobs` pages in flight;
    // a waiting encoder does not occupy a rendering worker.
    let next = AtomicUsize::new(0);
    let stopped = AtomicBool::new(false);
    let group = CancelGroup::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        let mut failure: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for worker in 0..jobs {
            let (pool, next, stopped, group) = (&pool, &next, &stopped, &group);
            let spawned = std::thread::Builder::new()
                .name(format!("png-{worker}"))
                .spawn_scoped(scope, move || {
                    let result = (|| -> CliResult<()> {
                        while !stopped.load(Ordering::Acquire) {
                            let Some(&index) = pages.get(next.fetch_add(1, Ordering::Relaxed))
                            else {
                                break;
                            };
                            let ticket = pool.render_in_group(
                                index,
                                config.clone(),
                                Priority::Visible,
                                group,
                            );
                            if stopped.load(Ordering::Acquire) {
                                ticket.cancel();
                            }
                            let result = ticket.wait()?;
                            let path = options.output.join(format!("page-{:04}.png", index + 1));
                            result.bitmap.write_png(&path)?;
                            println!("{}", path.display());
                        }
                        Ok(())
                    })();
                    if result.is_err() {
                        stopped.store(true, Ordering::Release);
                        group.cancel_all();
                    }
                    result
                });
            match spawned {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    stopped.store(true, Ordering::Release);
                    group.cancel_all();
                    failure = Some(Box::new(error));
                    break;
                }
            }
        }
        for handle in handles {
            let result = handle
                .join()
                .unwrap_or_else(|_| Err("PNG worker panicked".into()));
            if let Err(error) = result {
                stopped.store(true, Ordering::Release);
                group.cancel_all();
                // Prefer the actual failure over peer cancellation caused by it.
                if failure.is_none()
                    || failure.as_ref().is_some_and(|e| {
                        matches!(
                            e.downcast_ref::<pdfoxium::Error>(),
                            Some(pdfoxium::Error::Cancelled)
                        )
                    })
                {
                    failure = Some(error);
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    })
}

pub fn run_args(args: impl IntoIterator<Item = OsString>) -> CliResult<()> {
    match parse_args(args)
        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })?
    {
        None => {
            print!("{HELP}");
            Ok(())
        }
        Some(options) => run(options),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        parse_args(args.iter().map(|arg| OsString::from(*arg)))
    }

    #[test]
    fn parallel_cli_matches_serial_and_propagates_failures() {
        struct Temp(PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let temp = Temp(std::env::temp_dir().join(format!(
            "pdfoxium-render-jobs-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )));
        std::fs::create_dir_all(&temp.0).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/render_corpus/pdfs/rectangles_multi_pages.pdf");
        let options = |jobs, output| Options {
            input: fixture.clone(),
            output,
            jobs,
            dpi: 36.0,
            pages: Some(vec![(1, 3), (2, 2)]),
            password: None,
            rotation: Rotation::Cw90,
            grayscale: true,
            annotations: false,
        };
        let serial = temp.0.join("serial");
        let parallel = temp.0.join("parallel");
        run(options(1, serial.clone())).unwrap();
        // More requested jobs than selected pages also exercises worker capping.
        run(options(8, parallel.clone())).unwrap();
        assert_eq!(std::fs::read_dir(&parallel).unwrap().count(), 3);
        for index in 1..=3 {
            let name = format!("page-{index:04}.png");
            assert_eq!(
                std::fs::read(serial.join(&name)).unwrap(),
                std::fs::read(parallel.join(&name)).unwrap()
            );
        }
        let invalid = temp.0.join("invalid");
        let mut bad = options(4, invalid.clone());
        bad.pages = Some(vec![(999, 999)]);
        assert!(run(bad).is_err());
        assert!(!invalid.exists());

        // An output write failure drops the pool and joins every worker.
        let failed = temp.0.join("failed");
        std::fs::create_dir_all(failed.join("page-0001.png")).unwrap();
        assert!(run(options(4, failed)).is_err());

        let mut encrypted = options(4, temp.0.join("encrypted"));
        encrypted.input = fixture.with_file_name("encrypted_r6.pdf");
        encrypted.pages = Some(vec![(1, 1)]);
        encrypted.password = Some("incorrect".into());
        assert!(run(encrypted).is_err());
        let mut encrypted = options(4, temp.0.join("encrypted-ok"));
        encrypted.input = fixture.with_file_name("encrypted_r6.pdf");
        encrypted.pages = Some(vec![(1, 1)]);
        encrypted.password = Some("âge".into());
        run(encrypted).unwrap();
    }

    #[test]
    fn selects_one_based_ranges_once_in_page_order() {
        let ranges = parse_pages("7, 1-3,2, 5-5").unwrap();
        assert_eq!(selected_pages(Some(&ranges), 7).unwrap(), [0, 1, 2, 4, 6]);
        assert_eq!(selected_pages(None, 3).unwrap(), [0, 1, 2]);
        assert!(selected_pages(None, 0).unwrap().is_empty());
    }

    #[test]
    fn rejects_invalid_ranges_without_expanding_large_ones() {
        for value in [
            "", "0", "-1", "+1", "1-", "-3", "3-1", "1-2-3", "1,", "1,,2",
        ] {
            assert!(parse_pages(value).is_err(), "accepted {value:?}");
        }
        assert!(parse_pages("999999999999999999999999999999999999999").is_err());
        let ranges = parse_pages(&format!("1-{}", usize::MAX)).unwrap();
        assert!(selected_pages(Some(&ranges), 2).is_err());
        assert!(selected_pages(Some(&[(1, 1)]), 0).is_err());
        assert!(selected_pages(Some(&[(2, 3)]), 2).is_err());
    }

    #[test]
    fn parses_options_and_defaults() {
        let defaults = parse(&["input.pdf"]).unwrap().unwrap();
        assert_eq!(defaults.input, PathBuf::from("input.pdf"));
        assert_eq!(defaults.output, PathBuf::from("rendered"));
        assert_eq!(defaults.dpi, 144.0);
        assert_eq!(defaults.jobs, 1);
        assert_eq!(parse(&["input.pdf", "-j", "4"]).unwrap().unwrap().jobs, 4);
        assert!(defaults.annotations);
        assert!(!defaults.grayscale);

        let options = parse(&[
            "input.pdf",
            "-o",
            "out",
            "--dpi",
            "72",
            "--pages",
            "1,3-4",
            "--rotation",
            "90",
            "--grayscale",
            "--no-annotations",
            "--password",
            "secret",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(options.output, PathBuf::from("out"));
        assert_eq!(options.dpi, 72.0);
        assert_eq!(options.pages.unwrap(), [(1, 1), (3, 4)]);
        assert_eq!(options.rotation, Rotation::Cw90);
        assert!(options.grayscale);
        assert!(!options.annotations);
        assert_eq!(options.password.as_deref(), Some("secret"));
        assert_eq!(
            parse(&["--", "-input.pdf"]).unwrap().unwrap().input,
            PathBuf::from("-input.pdf")
        );
        assert!(parse(&["--help"]).unwrap().is_none());
    }

    #[test]
    fn rejects_bad_options_before_opening_input() {
        for args in [
            vec![],
            vec![""],
            vec!["a.pdf", "b.pdf"],
            vec!["a.pdf", "--unknown"],
            vec!["a.pdf", "--jobs"],
            vec!["a.pdf", "--jobs", "0"],
            vec!["a.pdf", "--jobs", "-1"],
            vec!["a.pdf", "--jobs", "1.5"],
            vec!["a.pdf", "--jobs", "999999999999999999999999"],
            vec!["a.pdf", "--jobs", "2", "-j", "3"],
            vec!["a.pdf", "--dpi"],
            vec!["a.pdf", "--dpi", "0"],
            vec!["a.pdf", "--dpi", "-1"],
            vec!["a.pdf", "--dpi", "NaN"],
            vec!["a.pdf", "--dpi", "inf"],
            vec!["a.pdf", "--dpi", "1e100"],
            vec!["a.pdf", "--rotation", "45"],
            vec!["a.pdf", "--pages", ""],
            vec!["a.pdf", "--output", ""],
            vec!["a.pdf", "--dpi", "72", "--dpi", "144"],
        ] {
            assert!(parse(&args).is_err(), "accepted {args:?}");
        }
    }
}
