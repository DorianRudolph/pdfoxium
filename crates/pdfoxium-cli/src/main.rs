// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

#![forbid(unsafe_code)]

mod render;

use pdfoxium::{Document, SearchOptions};
use std::{
    ffi::OsString,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicU64, Ordering},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const HELP: &str = "PDF inspection, rendering and assembly.

Usage:
  pdfoxium info INPUT.pdf [--password TEXT]
  pdfoxium render INPUT.pdf [--output DIR] [--jobs N] [--pages LIST] [--dpi N]
  pdfoxium text INPUT.pdf [--pages LIST] [--password TEXT]
  pdfoxium search INPUT.pdf TERM [--pages LIST] [--match-case] [--whole-word] [--rects]
  pdfoxium merge INPUT.pdf INPUT.pdf ... --output OUTPUT.pdf [--password TEXT]
  pdfoxium split INPUT.pdf --pages LIST --output OUTPUT.pdf [--password TEXT]

Page lists are one-based inclusive ranges, e.g. 1-3,7. Text and search preserve
list order and duplicates; split also preserves them in the output document.
Render selects each page once in page order. Use `pdfoxium render --help` for
all render options. Text writes UTF-8; search prints page, byte range and quoted
matched text, optionally followed by highlight rectangles in page units.

Merge's password applies to all inputs. Merge/split publish the output only
once saving succeeds; existing output files are replaced. Input paths cannot
be used as output paths. Use -- before positional values starting with '-'.
";

#[derive(Default)]
struct Args {
    inputs: Vec<OsString>,
    output: Option<PathBuf>,
    pages: Option<Vec<(usize, usize)>>,
    password: Option<String>,
    match_case: bool,
    whole_word: bool,
    rects: bool,
}

fn utf8(value: OsString, label: &str) -> Result<String> {
    value
        .into_string()
        .map_err(|_| format!("{label} must be UTF-8").into())
}

fn parse_pages(value: &str) -> Result<Vec<(usize, usize)>> {
    let number = |value: &str| -> Result<usize> {
        let value = value.trim();
        if value.is_empty() || !value.bytes().all(|ch| ch.is_ascii_digit()) {
            return Err("page numbers must be positive integers".into());
        }
        let value: usize = value.parse()?;
        if value == 0 {
            return Err("page numbers start at 1".into());
        }
        Ok(value)
    };
    value
        .split(',')
        .map(|part| {
            let (first, last) = match part.split_once('-') {
                Some((first, last)) => (number(first)?, number(last)?),
                None => {
                    let page = number(part)?;
                    (page, page)
                }
            };
            if first > last {
                return Err("page ranges must be ascending".into());
            }
            Ok((first, last))
        })
        .collect()
}

fn selection(ranges: Option<&[(usize, usize)]>, count: usize) -> Result<Vec<usize>> {
    let Some(ranges) = ranges else {
        return Ok((0..count).collect());
    };
    let mut total = 0usize;
    for &(first, last) in ranges {
        if first == 0 || first > last || last > count {
            return Err(format!(
                "page range {first}-{last} is outside this document's {count} pages"
            )
            .into());
        }
        total = total
            .checked_add(last - first + 1)
            .ok_or("page selection is too large")?;
    }
    let mut result = Vec::new();
    result.try_reserve(total)?;
    for &(first, last) in ranges {
        result.extend(first - 1..last);
    }
    Ok(result)
}

fn parse(command: &str, args: impl IntoIterator<Item = OsString>) -> Result<Option<Args>> {
    let mut result = Args::default();
    let mut args = args.into_iter();
    let mut positional = false;
    while let Some(value) = args.next() {
        if !positional {
            let flag = value.to_str().unwrap_or("");
            let mut next = || {
                args.next()
                    .ok_or_else(|| format!("missing value for {flag}"))
            };
            match flag {
                "--" => {
                    positional = true;
                    continue;
                }
                "-h" | "--help" => return Ok(None),
                "--password" => {
                    if result.password.is_some() {
                        return Err("duplicate --password".into());
                    }
                    result.password = Some(utf8(next()?, flag)?);
                    continue;
                }
                "--pages" if matches!(command, "text" | "search" | "split") => {
                    if result.pages.is_some() {
                        return Err("duplicate --pages".into());
                    }
                    result.pages = Some(parse_pages(&utf8(next()?, flag)?)?);
                    continue;
                }
                "-o" | "--output" if matches!(command, "merge" | "split") => {
                    if result.output.is_some() {
                        return Err("duplicate --output".into());
                    }
                    let path = PathBuf::from(next()?);
                    if path.as_os_str().is_empty() {
                        return Err("output path is empty".into());
                    }
                    result.output = Some(path);
                    continue;
                }
                "--match-case" if command == "search" => {
                    result.match_case = true;
                    continue;
                }
                "--whole-word" if command == "search" => {
                    result.whole_word = true;
                    continue;
                }
                "--rects" if command == "search" => {
                    result.rects = true;
                    continue;
                }
                _ if flag.starts_with('-') => {
                    return Err(format!("unknown option {flag:?} for {command}").into())
                }
                _ => {}
            }
        }
        if value.is_empty() {
            return Err("empty positional argument".into());
        }
        result.inputs.push(value);
    }
    let correct_inputs = match command {
        "search" => result.inputs.len() == 2,
        "merge" => !result.inputs.is_empty(),
        _ => result.inputs.len() == 1,
    };
    if !correct_inputs {
        return Err(format!("wrong number of inputs for {command}; use --help").into());
    }
    if matches!(command, "merge" | "split") && result.output.is_none() {
        return Err("--output is required".into());
    }
    if command == "split" && result.pages.is_none() {
        return Err("--pages is required".into());
    }
    Ok(Some(result))
}

fn open(path: &OsString, args: &Args) -> pdfoxium::Result<Document> {
    Document::open_with_password(path, args.password.as_deref().unwrap_or(""))
}

fn info(document: &Document, writer: &mut impl Write) -> Result<()> {
    let version = document.version()?;
    writeln!(writer, "PDF {}.{}", version.major, version.minor)?;
    writeln!(writer, "Pages: {}", document.page_count()?)?;
    let metadata = document.metadata()?;
    for (label, value) in [
        ("Title", metadata.title),
        ("Author", metadata.author),
        ("Subject", metadata.subject),
        ("Keywords", metadata.keywords),
        ("Creator", metadata.creator),
        ("Producer", metadata.producer),
        ("Creation date", metadata.creation_date),
        ("Modification date", metadata.modification_date),
    ] {
        if let Some(value) = value {
            writeln!(writer, "{label}: {value}")?;
        }
    }
    writeln!(
        writer,
        "Permissions: {:#010x}",
        document.permissions()?.bits
    )?;
    writeln!(writer, "Form: {:?}", document.form_type()?)?;
    for (index, layout) in document.page_layouts()?.iter().enumerate() {
        writeln!(
            writer,
            "Page {}: {} x {} page units, rotation {:?}, label {:?}",
            index + 1,
            layout.size.width,
            layout.size.height,
            layout.rotation,
            layout.label
        )?;
    }
    for attachment in document.attachments()? {
        writeln!(writer, "Attachment: {:?}", attachment.name())?;
    }
    for item in document.outline()? {
        writeln!(writer, "Outline {}: {}", item.depth, item.title)?;
    }
    Ok(())
}

static NEXT_OUTPUT: AtomicU64 = AtomicU64::new(0);

fn save_output(document: &Document, output: &Path, inputs: &[OsString]) -> Result<()> {
    let canonical_inputs: Vec<_> = inputs
        .iter()
        .map(std::fs::canonicalize)
        .collect::<io::Result<_>>()?;
    match std::fs::canonicalize(output) {
        Ok(existing) if canonical_inputs.contains(&existing) => {
            return Err("output would replace an input PDF".into())
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    struct Temporary {
        path: PathBuf,
        published: bool,
    }
    impl Drop for Temporary {
        fn drop(&mut self) {
            if !self.published {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
    let (mut temporary, mut file) = loop {
        let id = NEXT_OUTPUT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".pdfoxium-{}-{id}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                break (
                    Temporary {
                        path,
                        published: false,
                    },
                    file,
                )
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    document.save_to(&mut file)?;
    file.flush()?;
    drop(file);
    std::fs::rename(&temporary.path, output)?;
    temporary.published = true;
    Ok(())
}

fn execute(command: &str, args: Args) -> Result<()> {
    match command {
        "merge" | "split" => {
            // Open and validate every source before creating any output file.
            let documents = args
                .inputs
                .iter()
                .map(|input| open(input, &args))
                .collect::<pdfoxium::Result<Vec<_>>>()?;
            let selections = documents
                .iter()
                .map(|document| selection(args.pages.as_deref(), document.page_count()?))
                .collect::<Result<Vec<_>>>()?;
            let output = Document::create()?;
            for (document, pages) in documents.iter().zip(selections) {
                output.import_pages(document, &pages, output.page_count()?)?;
            }
            save_output(&output, args.output.as_ref().unwrap(), &args.inputs)
        }
        "info" => info(&open(&args.inputs[0], &args)?, &mut io::stdout().lock()),
        "text" | "search" => {
            let query = if command == "search" {
                let query = utf8(args.inputs[1].clone(), "search term")?;
                if query.is_empty() {
                    return Err("search term must not be empty".into());
                }
                Some(query)
            } else {
                None
            };
            let document = open(&args.inputs[0], &args)?;
            let pages = selection(args.pages.as_deref(), document.page_count()?)?;
            let mut writer = io::stdout().lock();
            let options = SearchOptions {
                match_case: args.match_case,
                match_whole_word: args.whole_word,
                ..Default::default()
            };
            for (position, index) in pages.into_iter().enumerate() {
                let page = document.page(index)?;
                let text = page.text()?;
                if let Some(query) = &query {
                    if args.rects {
                        for hit in text.search_with_rects(query, options)? {
                            let hit = hit?;
                            writeln!(
                                writer,
                                "{}\t{:?}\t{:?}\t{:?}",
                                index + 1,
                                hit.byte_range(),
                                hit.text(),
                                hit.rects
                            )?;
                        }
                    } else {
                        for hit in text.search(query, options)? {
                            let hit = hit?;
                            writeln!(
                                writer,
                                "{}\t{:?}\t{:?}",
                                index + 1,
                                hit.byte_range(),
                                hit.text()
                            )?;
                        }
                    }
                } else {
                    if position != 0 {
                        writer.write_all(b"\n\x0c\n")?;
                    }
                    writer.write_all(text.as_str().as_bytes())?;
                }
                drop(text);
                page.unload()?;
            }
            Ok(())
        }
        _ => Err(format!("unknown command {command:?}").into()),
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let Some(command) = args.next() else {
        print!("{HELP}");
        return Ok(());
    };
    if matches!(command.to_str(), Some("--help" | "-h")) {
        print!("{HELP}");
        return Ok(());
    }
    let command = utf8(command, "command")?;
    if command == "render" {
        return render::run_args(args);
    }
    if !matches!(
        command.as_str(),
        "info" | "text" | "search" | "merge" | "split"
    ) {
        return Err(format!("unknown command {command:?}; use --help").into());
    }
    match parse(&command, args)? {
        Some(args) => execute(&command, args),
        None => {
            print!("{HELP}");
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pdfoxium: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_matching_temporary_name_survives_publication() {
        struct Directory(PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let directory = Directory(std::env::temp_dir().join(format!(
            "pdfoxium-publish-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        )));
        std::fs::create_dir(&directory.0).unwrap();
        // This is the only unit test calling save_output. No other test uses
        // this process's output counter, so the next sibling name is known.
        let output = directory.0.join(format!(
            ".pdfoxium-{}-{}.tmp",
            std::process::id(),
            NEXT_OUTPUT.load(Ordering::Relaxed)
        ));
        let document = Document::open(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/render_corpus/pdfs/hello_world.pdf"),
        )
        .unwrap();
        save_output(&document, &output, &[]).unwrap();
        assert_eq!(Document::open(&output).unwrap().page_count().unwrap(), 1);
    }
}
