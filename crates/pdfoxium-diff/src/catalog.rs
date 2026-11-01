// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Shared input catalog/selection for fetching, rendering and benchmarking.
use crate::{project_root, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Expectation {
    #[default]
    Render,
    Reject,
    NoCrash,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Case {
    pub id: String,
    pub path: PathBuf,
    pub page_count: Option<usize>,
    #[serde(default)]
    pub page: Option<usize>,
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    pub password: Option<String>,
    #[serde(default)]
    pub flags: i32,
    pub background_bgra: Option<String>,
    pub first_page: Option<usize>,
    pub last_page: Option<usize>,
    #[serde(default)]
    pub all_pages: bool,
    #[serde(default)]
    pub groups: Vec<String>,
    pub download_url: Option<String>,
    #[serde(default)]
    pub upstream_ignored: bool,
    pub notes: Option<String>,
    pub suite: Option<String>,
    #[serde(default)]
    pub expectation: Expectation,
}
fn default_dpi() -> u32 {
    72
}

impl Case {
    pub fn run_id(&self) -> String {
        // Preserve existing bundled cache paths. New namespaced inputs use a
        // separate settings component, avoiding filename/DPI suffix collisions.
        let mut id = if self.id.contains('/') {
            format!("{}/dpi-{}", self.id, self.dpi)
        } else if self.dpi == 72 {
            self.id.clone()
        } else {
            format!("{}-dpi-{}", self.id, self.dpi)
        };
        if self.all_pages && (self.first_page.is_some() || self.last_page.is_some()) {
            id.push_str("/all-pages");
        }
        id
    }
    pub fn input_path(&self) -> PathBuf {
        project_root().join(&self.path)
    }
    pub fn selected_pages(&self, count: usize) -> Result<std::ops::Range<usize>> {
        if let Some(page) = self.page {
            return if page < count {
                Ok(page..page + 1)
            } else {
                Err(format!("page {page} out of range for {count} pages").into())
            };
        }
        let first = if self.all_pages {
            0
        } else {
            self.first_page.unwrap_or(0)
        };
        let end = if self.all_pages {
            count
        } else {
            self.last_page
                .map(|v| v.checked_add(1).ok_or("last page overflow"))
                .transpose()?
                .unwrap_or(count)
        };
        if first >= end || end > count {
            return Err(format!("page range {first}..{end} out of range for {count} pages").into());
        }
        Ok(first..end)
    }
    pub fn page_description(&self) -> String {
        if let Some(page) = self.page {
            return page.to_string();
        }
        if self.all_pages || (self.first_page.is_none() && self.last_page.is_none()) {
            return "all".into();
        }
        format!(
            "{}..={}",
            self.first_page.unwrap_or(0),
            self.last_page
                .map_or_else(|| "end".into(), |p| p.to_string())
        )
    }
}

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

pub fn catalog() -> Result<Vec<Case>> {
    let mut bundled: Corpus = serde_json::from_slice(&fs::read(
        project_root().join("tests/render_corpus/manifest.json"),
    )?)?;
    if bundled.cases.is_empty()
        || bundled
            .cases
            .iter()
            .any(|c| c.page_count.is_none_or(|n| n == 0))
    {
        return Err("bundled corpus must list positive expected page counts".into());
    }
    for c in &mut bundled.cases {
        // `all` retains the full primary catalog, including opt-in external
        // inputs. `bundled` means the small fixtures actually shipped in Git.
        c.groups.push("all".into());
        if c.download_url.is_none() && !c.groups.iter().any(|g| g == "local-only") {
            c.groups.push("bundled".into());
        }
        if let Some(suite) = &c.suite {
            c.groups.push(suite.clone());
        }
    }
    let hayro: Corpus = serde_json::from_slice(&fs::read(
        project_root().join("tests/render_corpus/hayro.json"),
    )?)?;
    bundled.cases.extend(hayro.cases);
    let mut ids = BTreeSet::new();
    for c in &bundled.cases {
        if !safe_id(&c.id)
            || !ids.insert(c.id.clone())
            || c.page_count == Some(0)
            || c.first_page.zip(c.last_page).is_some_and(|(a, b)| a > b)
        {
            return Err(format!("invalid or duplicate catalog case: {}", c.id).into());
        }
    }
    Ok(bundled.cases)
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('\\')
        && id
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
        && !Path::new(id).is_absolute()
}

/// Canonical path components are escaped, never flattened to a filename stem.
/// Includes the absolute root so inputs outside the repository cannot collide.
pub fn local_id(path: &Path) -> Result<String> {
    let path = fs::canonicalize(path)?;
    let mut parts = vec!["local".to_string()];
    for component in path.components() {
        if component == Component::RootDir {
            continue;
        }
        let text = component
            .as_os_str()
            .to_str()
            .ok_or("PDF path is not UTF-8")?;
        let mut escaped = String::new();
        for b in text.bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                escaped.push(b as char);
            } else {
                escaped.push_str(&format!("%{b:02X}"));
            }
        }
        parts.push(escaped);
    }
    Ok(parts.join("/"))
}
fn path_case(path: &Path, cases: &[Case]) -> Result<Case> {
    let path = fs::canonicalize(path)?;
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
    {
        return Err(format!("not a PDF file: {}", path.display()).into());
    }
    if let Some(case) = cases
        .iter()
        .find(|c| c.input_path().canonicalize().ok().as_ref() == Some(&path))
    {
        return Ok(case.clone());
    }
    Ok(Case {
        id: local_id(&path)?,
        path,
        page_count: None,
        page: None,
        dpi: 72,
        password: None,
        flags: 0,
        background_bgra: None,
        first_page: None,
        last_page: None,
        all_pages: false,
        groups: vec![],
        download_url: None,
        upstream_ignored: false,
        notes: None,
        suite: None,
        expectation: Expectation::Render,
    })
}
fn directory_pdfs(path: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let kind = entry.file_type()?;
        // Do not follow directory symlinks: workspace scans must not loop or
        // silently traverse unrelated trees. Explicit file symlinks still work.
        if kind.is_dir() {
            directory_pdfs(&entry.path(), result)?;
        } else if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
        {
            result.push(entry.path());
        }
    }
    Ok(())
}

pub struct Selection {
    pub cases: Vec<Case>,
    pub unavailable: usize,
}

pub fn select(selectors: &[String], available: bool) -> Result<Selection> {
    select_from(&catalog()?, selectors, available)
}
fn select_from(catalog: &[Case], selectors: &[String], available: bool) -> Result<Selection> {
    if selectors.is_empty() {
        return Err("specify a set, case ID, PDF or directory; use --help".into());
    }
    let mut selected = BTreeMap::new();
    let mut missing = BTreeSet::new();
    for selector in selectors {
        let mut filter_available = available;
        let cases: Vec<Case> = if selector == "workspace" {
            filter_available = true;
            catalog.to_vec()
        } else {
            let group = selector.as_str();
            let cases: Vec<_> = catalog
                .iter()
                .filter(|c| c.id == group || c.groups.iter().any(|s| s == group))
                .cloned()
                .collect();
            if !cases.is_empty() {
                cases
            } else {
                let path = Path::new(selector);
                if path.is_dir() {
                    let mut paths = Vec::new();
                    directory_pdfs(path, &mut paths)?;
                    paths
                        .iter()
                        .map(|p| path_case(p, catalog))
                        .collect::<Result<_>>()?
                } else {
                    vec![path_case(path, catalog)?]
                }
            }
        };
        for case in cases {
            if filter_available && !case.input_path().is_file() {
                missing.insert(case.id);
            } else {
                selected.insert(case.id.clone(), case);
            }
        }
    }
    // A second selector may explicitly include an otherwise filtered input.
    missing.retain(|id| !selected.contains_key(id));
    Ok(Selection {
        cases: selected.into_values().collect(),
        unavailable: missing.len(),
    })
}
pub fn select_case(selector: &str) -> Result<Case> {
    let mut selection = select(&[selector.to_string()], false)?;
    if selection.cases.len() != 1 {
        return Err("expected one PDF, not a set".into());
    }
    Ok(selection.cases.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_counts_ranges_passwords_and_upstream_ignores_are_preserved() {
        let all = catalog().unwrap();
        let count = |s: &str| select_from(&all, &[s.into()], false).unwrap().cases.len();
        assert_eq!(count("all"), 46);
        assert_eq!(count("bundled"), 43);
        assert_eq!(count("books"), 3);
        let books = select_from(&all, &["books".into()], false).unwrap().cases;
        assert!(books.iter().all(|c| c.download_url.is_some()
            && c.path.starts_with("tests/render_corpus/books")
            && !c.groups.iter().any(|g| g == "bundled")));
        let bundled = select_from(&all, &["bundled".into()], false).unwrap().cases;
        assert!(bundled.iter().all(|c| c.download_url.is_none()
            && (c.path.starts_with("tests/render_corpus/pdfs")
                || c.path == Path::new("tests/render_corpus/books/hamiltonian.pdf"))
            && c.input_path().is_file()));
        assert_eq!(count("hayro"), 1612);
        for (s, n) in [
            ("hayro/custom", 311),
            ("hayro/local", 286),
            ("hayro/pdfjs", 679),
            ("hayro/pdfbox", 454),
            ("hayro/pdfium", 127),
            ("hayro/corpus", 41),
        ] {
            assert_eq!(count(s), n);
        }
        let hayro: Vec<_> = all.iter().filter(|c| c.id.starts_with("hayro/")).collect();
        assert_eq!(hayro.iter().filter(|c| c.upstream_ignored).count(), 74);
        assert_eq!(hayro.iter().filter(|c| c.password.is_some()).count(), 13);
        assert_eq!(
            hayro
                .iter()
                .filter(|c| c.first_page.is_some() || c.last_page.is_some())
                .count(),
            301
        );
        assert!(hayro
            .iter()
            .all(|c| c.download_url.is_some() && c.page_count.is_none()));
    }
    #[test]
    fn range_overrides_and_cache_settings_remain_distinct() {
        let mut c = select_case("hayro/corpus/0344421").unwrap();
        assert_eq!(c.selected_pages(20).unwrap(), 7..8);
        assert!(c.selected_pages(7).is_err());
        let subset = c.run_id();
        c.all_pages = true;
        assert_eq!(c.selected_pages(20).unwrap(), 0..20);
        assert_ne!(c.run_id(), subset);
        c.page = Some(0);
        assert_eq!(c.selected_pages(20).unwrap(), 0..1);
        c.page = Some(20);
        assert!(c.selected_pages(20).is_err());
        c.page = None;
        c.all_pages = false;
        c.last_page = Some(usize::MAX);
        assert!(c.selected_pages(20).is_err());
        let mut c = select_case("blank").unwrap();
        assert_eq!(c.run_id(), "blank");
        c.dpi = 300;
        assert_eq!(c.run_id(), "blank-dpi-300");
    }
    #[test]
    fn availability_union_and_canonical_path_preserve_metadata() {
        let mut a = select_case("blank").unwrap();
        a.password = Some("fixture".into());
        a.first_page = Some(0);
        a.groups.push("sample".into());
        let mut b = a.clone();
        b.id = "absent".into();
        b.path = PathBuf::from("target/intentionally-absent-corpus.pdf");
        let catalog = [a.clone(), b];
        let selection = select_from(
            &catalog,
            &["workspace".into(), "sample".into(), "blank".into()],
            true,
        )
        .unwrap();
        assert_eq!((selection.cases.len(), selection.unavailable), (1, 1));
        let by_path = path_case(&a.input_path(), &catalog).unwrap();
        assert_eq!(by_path.password, a.password);
        assert_eq!(by_path.first_page, Some(0));
        for id in ["../x", "/absolute", "x/../y", "x\\y", "x//y"] {
            assert!(!safe_id(id));
        }
    }
}
