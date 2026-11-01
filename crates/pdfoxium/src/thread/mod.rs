// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Static rendering pools and a single owning worker for live form documents.
//!
//! [`RenderPool`] loads independent copies of saved bytes. [`DocumentThread`]
//! serializes form input, timers and rendering against one live document.
//!
//! ```no_run
//! use pdfoxium::{Config, RenderConfig};
//! use pdfoxium::thread::{RenderPool, Source, Priority};
//! # fn main() -> pdfoxium::Result<()> {
//! let pool = RenderPool::new(Source::Path("input.pdf".into()), Config::default(), 4)?;
//! let result = pool.render(0, RenderConfig::dpi(144.0), Priority::Visible).wait()?;
//! result.bitmap.write_png("page.png")?;
//! # Ok(()) }
//! ```

mod dialog;
mod document;
mod pool;
pub mod runtime;
mod ticket;
pub use document::{DocEvent, DocOptions, DocumentThread};
pub use pool::{PoolBitmap, PoolOptions, PoolSource, Priority, RenderPool, Source};
pub use ticket::{CancelGroup, Ticket};
