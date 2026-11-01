// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Detached navigation data; no document handles cross the reader thread.
use eframe::egui::{Pos2, Rect};
use pdfoxium::{Destination, Document, LinkTarget, Matrix, Point, RenderConfig};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct Link {
    pub index: usize,
    pub rect: Rect,
    pub target: Option<LinkTarget>,
}
#[derive(Clone, Debug)]
pub struct Resolved {
    pub destination: Destination,
    pub matrix: Matrix,
    pub current_matrix: Matrix,
    pub crop: pdfoxium::Rect,
}
pub fn links(document: &Document, index: usize) -> Result<Vec<Link>, String> {
    let page = document.page(index).map_err(|e| e.to_string())?;
    let result = (|| -> pdfoxium::Result<_> {
        let matrix = page.render_matrix(&RenderConfig::fit(1, 1))?;
        let ids = page.annotation_ids()?;
        let visible = ids
            .into_iter()
            .map(|id| page.annotation_layer_visible(id))
            .collect::<pdfoxium::Result<Vec<_>>>()?;
        Ok(page
            .links()?
            .into_iter()
            .filter(|link| visible.get(link.annotation_index).copied().unwrap_or(false))
            .map(|link| Link {
                index: link.annotation_index,
                rect: crate::text::normalized(matrix, link.rect),
                target: link.target,
            })
            .collect())
    })();
    let unloaded = page.unload();
    let result = result.map_err(|e| e.to_string())?;
    unloaded.map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn resolve(
    document: &Document,
    destination: Destination,
    current_page: usize,
) -> Result<Resolved, String> {
    let current = document.page(current_page).map_err(|e| e.to_string())?;
    let current_matrix = current.render_matrix(&RenderConfig::fit(1, 1));
    let unloaded = current.unload();
    let current_matrix = current_matrix.map_err(|e| e.to_string())?;
    unloaded.map_err(|e| e.to_string())?;
    let page = document
        .page(destination.page_index)
        .map_err(|e| e.to_string())?;
    let result = (|| -> pdfoxium::Result<_> {
        Ok(Resolved {
            destination,
            current_matrix,
            matrix: page.render_matrix(&RenderConfig::fit(1, 1))?,
            crop: page.layout()?.crop,
        })
    })();
    let unloaded = page.unload();
    let result = result.map_err(|e| e.to_string())?;
    unloaded.map_err(|e| e.to_string())?;
    Ok(result)
}
pub fn point(matrix: Matrix, x: f32, y: f32) -> Pos2 {
    let p = matrix.transform(Point { x, y });
    Pos2::new(p.x, p.y)
}
pub fn uri(bytes: &[u8]) -> Result<String, String> {
    let uri = std::str::from_utf8(bytes).map_err(|_| "link URI is not UTF-8")?;
    let (scheme, rest) = uri.split_once(':').ok_or("link URI has no scheme")?;
    if !["http", "https", "mailto"]
        .iter()
        .any(|s| scheme.eq_ignore_ascii_case(s))
        || rest.is_empty()
        || uri.chars().any(char::is_control)
    {
        return Err("only HTTP, HTTPS and mailto links can be opened".into());
    }
    Ok(uri.to_owned())
}
pub fn page_label(labels: &[Option<String>], page: usize) -> String {
    labels
        .get(page)
        .and_then(|label| label.as_deref())
        .map(str::to_owned)
        .unwrap_or_else(|| (page + 1).to_string())
}
pub fn page_caption(labels: &[Option<String>], page: usize) -> String {
    let label = page_label(labels, page);
    if label == (page + 1).to_string() {
        label
    } else {
        format!("{label} ({})", page + 1)
    }
}
pub fn destination_page(target: &Option<LinkTarget>) -> Option<usize> {
    match target {
        Some(LinkTarget::Destination(d))
        | Some(LinkTarget::NamedDestination {
            resolved: Some(d), ..
        }) => Some(d.page_index),
        _ => None,
    }
}
pub fn target_label(target: &Option<LinkTarget>) -> String {
    match target {
        Some(LinkTarget::Destination(d)) => format!("Page {}", d.page_index + 1),
        Some(LinkTarget::NamedDestination {
            resolved: Some(d), ..
        }) => format!("Page {}", d.page_index + 1),
        Some(LinkTarget::Uri(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        Some(LinkTarget::NamedAction(name)) => String::from_utf8_lossy(name).into_owned(),
        _ => "Unsupported or unresolved link".into(),
    }
}
pub struct History<T> {
    pub back: VecDeque<T>,
    pub forward: VecDeque<T>,
}
impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            back: VecDeque::new(),
            forward: VecDeque::new(),
        }
    }
}
impl<T: Clone + PartialEq> History<T> {
    pub fn record(&mut self, current: T) {
        if self.back.back() != Some(&current) {
            self.back.push_back(current);
            if self.back.len() > 100 {
                self.back.pop_front();
            }
        }
        self.forward.clear();
    }
    pub fn step(&mut self, current: T, backwards: bool) -> Option<T> {
        let (from, to) = if backwards {
            (&mut self.back, &mut self.forward)
        } else {
            (&mut self.forward, &mut self.back)
        };
        let target = from.pop_back()?;
        to.push_back(current);
        Some(target)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_restores_current_position_clears_branches_and_is_bounded() {
        let mut h = History::default();
        h.record(10);
        h.record(10);
        assert_eq!(h.step(20, true), Some(10));
        assert_eq!(h.step(11, false), Some(20));
        assert_eq!(h.step(21, true), Some(11));
        h.record(12);
        assert!(h.forward.is_empty());
        for i in 0..200 {
            h.record(i);
        }
        assert_eq!(h.back.len(), 100);
        assert_eq!(h.back.front(), Some(&100));
    }
    #[test]
    fn external_links_only_allow_supported_uri_schemes() {
        for value in [
            "https://example.org",
            "HTTP://example.org",
            "mailto:a@example.org",
        ] {
            assert!(uri(value.as_bytes()).is_ok());
        }
        for value in [
            "file:///tmp/a",
            "javascript:alert(1)",
            "https:",
            "https://x\n",
            "relative.pdf",
        ] {
            assert!(uri(value.as_bytes()).is_err());
        }
        assert!(uri(&[255]).is_err());
    }
}
