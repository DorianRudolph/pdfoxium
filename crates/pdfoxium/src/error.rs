// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use std::{fmt, io};

pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by the safe API. No process-global last-error state is used.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Io(io::Error),
    Password,
    Format,
    Security,
    PageOutOfRange { index: usize, count: usize },
    Stale,
    Busy,
    Cancelled,
    Poisoned,
    InvalidArgument(&'static str),
    Allocation,
    Render,
    Save,
    Import,
    Edit,
    Unsupported(&'static str),
    NotFinished,
    Internal(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Password => f.write_str("missing or incorrect PDF password"),
            Self::Format => f.write_str("invalid or unrecoverable PDF data"),
            Self::Security => f.write_str("unsupported PDF security handler"),
            Self::PageOutOfRange { index, count } => {
                write!(
                    f,
                    "page index {index} is outside a document with {count} pages"
                )
            }
            Self::Stale => f.write_str("identity does not belong to this live document"),
            Self::Cancelled => f.write_str("request was cancelled"),
            Self::Busy => f.write_str("a conflicting document or page operation is active"),
            Self::Poisoned => f.write_str("a core operation panicked; reopen the document"),
            Self::InvalidArgument(message) => write!(f, "invalid argument: {message}"),
            Self::Allocation => f.write_str("could not allocate the requested buffer"),
            Self::Render => f.write_str("page rendering failed"),
            Self::Edit => f.write_str("PDF editing operation failed"),
            Self::Unsupported(message) => write!(f, "unsupported operation: {message}"),
            Self::Import => {
                f.write_str("page import failed; destination may contain partially imported pages")
            }
            Self::Save => f.write_str("document saving failed"),
            Self::NotFinished => f.write_str("the render job has not completed"),
            Self::Internal(message) => write!(f, "internal invariant failed: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
