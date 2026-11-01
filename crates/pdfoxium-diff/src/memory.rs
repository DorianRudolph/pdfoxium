// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Persistent-document scrolling/zooming probe, without PNG or tile retention.
//! Each backend runs in its own process. Linux RSS includes allocator slack;
//! glibc in-use plus mmap bytes distinguish that from live heap allocations.

use super::font_extensions::FontExtensions;
use super::{library_path, project_root, render_page, select_case, selected_pages, Result};
use pdfium_render::prelude::{Pdfium, FPDF_PAGE};
use std::collections::VecDeque;
use std::io::{self, Write};

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod allocator {
    // glibc's public mallinfo2 ABI, all ten fields have C size_t type.
    #[repr(C)]
    struct MallInfo {
        arena: usize,
        ordblks: usize,
        smblks: usize,
        hblks: usize,
        hblkhd: usize,
        usmblks: usize,
        fsmblks: usize,
        uordblks: usize,
        fordblks: usize,
        keepcost: usize,
    }
    extern "C" {
        fn mallinfo2() -> MallInfo;
        fn malloc_trim(pad: usize) -> std::os::raw::c_int;
    }
    pub fn kib() -> (usize, usize, usize) {
        // SAFETY: A read-only snapshot from the process's glibc allocator.
        let info = unsafe { mallinfo2() };
        (
            (info.uordblks + info.hblkhd) / 1024,
            info.arena / 1024,
            info.hblkhd / 1024,
        )
    }

    pub fn trim() {
        // SAFETY: glibc's process-wide reclamation API takes only a pad size.
        // Diagnostic only, after document and library destruction.
        unsafe { malloc_trim(0) };
    }
}

struct Probe<'a> {
    output: io::BufWriter<io::StdoutLock<'a>>,
    backend: &'a str,
    case: String,
    extensions: Option<FontExtensions>,
    draws: u64,
    draw_ms: f64,
}

impl Probe<'_> {
    fn record(
        &mut self,
        phase: &str,
        pass: usize,
        page: Option<usize>,
        dpi: u32,
        open: usize,
    ) -> Result<()> {
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let (allocated, arena, mmap) = allocator::kib();
        #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
        let (allocated, arena, mmap) = (String::new(), String::new(), String::new());
        let status = std::fs::read_to_string("/proc/self/status")?;
        let mut font_fields = String::from(",,,");
        if let Some(ext) = &self.extensions {
            if let (Some(used), Some(limits)) = (ext.usage(), ext.limits()) {
                if used.0 > limits.0 || used.1 > limits.1 {
                    return Err("retained font memory exceeded configured limit".into());
                }
                font_fields = format!("{},{},{},{}", used.0, used.1, limits.0, limits.1);
            } else if !matches!(
                phase,
                "loaded_library" | "library_destroyed" | "allocator_trimmed"
            ) {
                return Err("font memory accounting unavailable while initialized".into());
            }
        }
        let field = |name: &str| -> Result<usize> {
            status
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|s| s.split_whitespace().next())
                .ok_or("missing Linux RSS field")?
                .parse()
                .map_err(Into::into)
        };
        writeln!(
            self.output,
            "{},{},{phase},{pass},{},{dpi},{open},{},{},{allocated},{arena},{mmap},{font_fields},{},{:.6}",
            self.backend,
            self.case,
            page.map_or_else(String::new, |v| v.to_string()),
            field("VmRSS:")?,
            field("VmHWM:")?, self.draws, self.draw_ms
        )?;
        self.output.flush()?;
        Ok(())
    }
}

pub(super) fn run(args: &[String]) -> Result<()> {
    if cfg!(debug_assertions) || !cfg!(target_os = "linux") {
        return Err("memory requires a Linux release build".into());
    }
    let backend = args.first().ok_or("missing memory backend")?.as_str();
    if !matches!(backend, "rust" | "native") {
        return Err("memory backend must be rust or native".into());
    }
    let mut case = select_case(args.get(1).ok_or("missing memory input")?)?;
    let (mut passes, mut window, mut zoom_steps, mut max_dpi) = (2usize, 3usize, 16usize, 300u32);
    let mut trim = false;
    let mut trim_fonts = false;
    let (mut cache_bytes, mut scratch_bytes) = (None, None);
    let mut options = args.iter().skip(2);
    while let Some(option) = options.next() {
        if option == "--trim-fonts" {
            trim_fonts = true;
            continue;
        }
        if option == "--trim" {
            if !cfg!(all(target_os = "linux", target_env = "gnu")) {
                return Err("--trim requires glibc".into());
            }
            trim = true;
            continue;
        }
        let value = options.next().ok_or("missing memory option value")?;
        match option.as_str() {
            "--iterations" => passes = value.parse()?,
            "--window" => window = value.parse()?,
            "--zoom-steps" => zoom_steps = value.parse()?,
            "--max-dpi" => max_dpi = value.parse()?,
            "--dpi" => case.dpi = value.parse()?,
            "--page" => case.page = Some(value.parse()?),
            "--cache-bytes" => cache_bytes = Some(value.parse::<u64>()?),
            "--scratch-bytes" => scratch_bytes = Some(value.parse::<u64>()?),
            _ => return Err("unknown memory option; use --help".into()),
        }
    }
    if passes == 0 || window == 0 || case.dpi == 0 || max_dpi < case.dpi {
        return Err("memory needs positive iterations/window/DPI and max DPI >= DPI".into());
    }
    let base_dpi = case.dpi;
    let stdout = io::stdout();
    let mut probe = Probe {
        output: io::BufWriter::new(stdout.lock()),
        backend,
        case: format!("\"{}\"", case.id.replace('"', "\"\"")),
        extensions: FontExtensions::load(backend),
        draws: 0,
        draw_ms: 0.0,
    };
    if (cache_bytes.is_some() || scratch_bytes.is_some() || trim_fonts)
        && probe.extensions.is_none()
    {
        return Err("requested font controls unavailable in this library".into());
    }
    writeln!(probe.output, "backend,case,phase,pass,page,dpi,open_pages,rss_kib,peak_rss_kib,allocated_kib,arena_kib,mmap_kib,glyph_cache_bytes,glyph_scratch_bytes,cache_limit_bytes,scratch_limit_bytes,draw_count,draw_total_ms")?;
    let bindings = Pdfium::bind_to_library(library_path(backend))?;
    probe.record("loaded_library", 0, None, 0, 0)?;
    // SAFETY: This process owns one initialized library. Close all retained
    // page handles before the document, including on errors. render_page owns
    // and destroys each output bitmap; this probe never retains pixel buffers.
    unsafe {
        bindings.FPDF_InitLibrary();
        let result = (|| -> Result<()> {
            if let Some(ext) = &probe.extensions {
                let limits = ext
                    .limits()
                    .ok_or("font limits unavailable after initialization")?;
                ext.set(
                    cache_bytes.unwrap_or(limits.0),
                    scratch_bytes.unwrap_or(limits.1),
                )?;
            }
            probe.record("initialized", 0, None, 0, 0)?;
            let path = project_root().join(&case.path);
            let document = bindings.FPDF_LoadDocument(
                path.to_str().ok_or("PDF path is not UTF-8")?,
                case.password.as_deref(),
            );
            if document.is_null() {
                return Err("memory document load failed".into());
            }
            let mut open: VecDeque<(usize, FPDF_PAGE)> = VecDeque::new();
            let result = (|| -> Result<()> {
                probe.record("document_loaded", 0, None, 0, 0)?;
                let count = bindings.FPDF_GetPageCount(document);
                if count <= 0 || case.page_count.is_some_and(|n| n != count as usize) {
                    return Err("invalid memory page count".into());
                }
                let pages = selected_pages(&case, count as usize)?;
                for pass in 1..=passes {
                    for index in pages.clone() {
                        if open.len() == window {
                            bindings.FPDF_ClosePage(open.pop_front().unwrap().1);
                        }
                        let page = bindings.FPDF_LoadPage(document, index as i32);
                        if page.is_null() {
                            return Err(format!("memory page {index}: load failed").into());
                        }
                        open.push_back((index, page));
                        let timing = render_page(bindings.as_ref(), page, None, &case)?;
                        probe.draws += 1;
                        probe.draw_ms += timing.render_ms;
                        if (index - pages.start + 1) % 100 == 0 {
                            probe.record("scroll", pass, Some(index), base_dpi, open.len())?;
                        }
                    }
                    probe.record("scroll_end", pass, None, base_dpi, open.len())?;
                    while let Some((_, page)) = open.pop_front() {
                        bindings.FPDF_ClosePage(page);
                    }
                    probe.record("scroll_pages_closed", pass, None, base_dpi, 0)?;
                    if trim_fonts {
                        probe.extensions.as_ref().unwrap().trim()?;
                        probe.record("fonts_trimmed", pass, None, base_dpi, 0)?;
                    }
                }
                // Up to eight evenly spaced anchors, or the explicitly selected
                // page. Keep each page live throughout its full zoom sequence.
                let anchors: Vec<_> = pages
                    .clone()
                    .step_by(pages.len().div_ceil(8).max(1))
                    .take(8)
                    .collect();
                for pass in 1..=passes {
                    if zoom_steps == 0 {
                        break;
                    }
                    for &index in &anchors {
                        let page = bindings.FPDF_LoadPage(document, index as i32);
                        if page.is_null() {
                            return Err("memory zoom page load failed".into());
                        }
                        open.push_back((index, page));
                        for step in 0..zoom_steps {
                            case.dpi = base_dpi
                                + (u64::from(max_dpi - base_dpi) * step as u64
                                    / zoom_steps.saturating_sub(1).max(1) as u64)
                                    as u32;
                            let timing = render_page(bindings.as_ref(), page, None, &case)?;
                            probe.draws += 1;
                            probe.draw_ms += timing.render_ms;
                            probe.record("zoom", pass, Some(index), case.dpi, open.len())?;
                        }
                        bindings.FPDF_ClosePage(open.pop_front().unwrap().1);
                    }
                    probe.record("zoom_pages_closed", pass, None, case.dpi, 0)?;
                    if trim_fonts {
                        probe.extensions.as_ref().unwrap().trim()?;
                        probe.record("fonts_trimmed", pass, None, case.dpi, 0)?;
                    }
                }
                Ok(())
            })();
            while let Some((_, page)) = open.pop_front() {
                bindings.FPDF_ClosePage(page);
            }
            bindings.FPDF_CloseDocument(document);
            probe.record("document_closed", 0, None, 0, 0)?;
            result
        })();
        bindings.FPDF_DestroyLibrary();
        probe.record("library_destroyed", 0, None, 0, 0)?;
        if trim {
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            allocator::trim();
            probe.record("allocator_trimmed", 0, None, 0, 0)?;
        }
        result
    }
}
