// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

mod annot;
mod bitmap;
mod config;
mod document;
mod edit;
mod error;
mod form_dialog;
mod forms;
pub use form_dialog::{FormDialog, FormDialogResponse};
mod geometry;
pub use forms::{
    FormDocumentAction, FormEvent, FormField, FormInput, FormInputResult, FormKey, FormModifiers,
    FormOptions, FormSession, FormState,
};
mod media;
mod page_edit;
mod page_object;
mod read;
mod render;
mod text;
pub mod thread;
pub mod view;

#[cfg(feature = "capi-support")]
#[doc(hidden)]
pub mod engine;
#[cfg(not(feature = "capi-support"))]
mod engine;

pub use bitmap::{Bitmap, Color, PixelFormat, Pixels, PixelsMut};
pub use config::{
    font_memory_limits, font_memory_usage, image_memory_limits, set_font_memory_limits,
    set_image_memory_limits, trim_font_memory, Config, FontMemoryLimits, FontMemoryUsage,
    ImageMemoryLimits,
};
pub(crate) use document::PageLease;
pub use document::{Document, Page, PageId, Pages, SaveOptions, Version};
pub use error::{Error, Result};
pub use render::{pause, DeviceRect, Progress, RenderConfig, RenderJob, RenderOutput, Rotation};

pub use annot::{AnnotId, AnnotationSpec, AnnotationStyle};
pub use edit::{
    FillRule, Font, FontType, ObjectId, ObjectInfo, ObjectKind, PathCommand, PathSpec,
    StandardFont, Stroke, TextSpec,
};
pub use geometry::{Matrix, Point, Rect, Size};
pub use media::{FontInfo, ImageInfo, JpegStorage, TextObjectData, TextRenderMode};
pub use page_edit::{ClipContour, ClipPath, FlattenUsage, PageBox, PageXObject};
pub use page_object::PageObject;

pub use read::{
    AnnotationInfo, Attachment, Destination, DestinationView, FormType, Link, LinkTarget, Metadata,
    OutlineItem, PageLayout, Permissions,
};

pub use read::{DocumentModel, PageModel};
pub use text::{
    DocumentSearch, DocumentSearchHit, DocumentSearchHitWithRects, DocumentSearchWithRects, Search,
    SearchHit, SearchHitWithRects, SearchOptions, SearchWithRects, TextPage, TextPosition,
    TextSelection,
};

pub use edit::{
    DetachedObject, LineCap, LineJoin, ObjectStyle, PathData, PathSegment, PathSegmentKind,
};

pub use annot::{
    AnnotationDetails, AnnotationKind, AnnotationObject, AnnotationObjectId, AnnotationObjectInfo,
    AnnotationValueType, AppearanceMode,
};
pub use edit::{ContentMark, MarkParameter, MarkValue};

pub use annot::{FieldAction, WidgetFieldInfo, WidgetFieldType, WidgetOption};

/// PDF object category used by typed dictionary queries.
pub use annot::AnnotationValueType as PdfValueType;

mod optional_content;
pub use optional_content::{
    OptionalContent, OptionalContentConfiguration, OptionalContentConfigurationId,
    OptionalContentEvent, OptionalContentGroup, OptionalContentGroupId, OptionalContentOrder,
    OptionalContentSnapshot, OptionalContentState, UsageEnvironment,
};

pub use pdfoxium_core::fpdfapi::page::optional_content::{
    OcAction as OptionalContentAction, OcOperation as OptionalContentOperation,
};
