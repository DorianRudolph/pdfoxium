// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

//! Face metadata and charmaps from `core/fxge/cfx_face.{h,cpp}`
//! and its Fontations backend in `core/fxge/skrifa/src/main.rs`.
//! Fontations outlines are rendered with the shared AGG rasterizer.

use super::font_memory::{FontMemory, Scratch, ScratchResource};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use kurbo::{expand_path, Diagonal2, Join, PathEl, Point};
use read_fonts::{
    ps::UnicodeCharmap,
    tables::{
        cmap::{Cmap2, CmapSubtable, SubHeader},
        head::MacStyle,
        os2::SelectionFlags,
    },
    types::{GlyphId, Tag},
    TableProvider,
};
use skrifa::{
    instance::{LocationRef, Size},
    outline::{
        DrawSettings, Engine, HintingInstance, HintingOptions, OutlineGlyphCollection,
        OutlineGlyphFormat, OutlinePen, Target,
    },
    string::StringId,
    FontRef, GlyphNameSource, MetadataProvider,
};

use crate::agg::agg_color_gray::Gray8;
use crate::agg::agg_curves::Curve4;
use crate::agg::agg_path_storage::PathStorage;
use crate::agg::agg_pixfmt_gray::PixfmtGray8;
use crate::agg::agg_rasterizer_scanline_aa::RasterizerScanlineAa;
use crate::agg::agg_render_scanlines::render_scanlines;
use crate::agg::agg_renderer_base::RendererBaseGray;
use crate::agg::agg_rendering_buffer::RenderingBuffer;
use crate::agg::agg_scanline_u::ScanlineU8;
use crate::fxcrt::fx_coordinates::CfxPoint;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, FxRect};
use crate::fxge::agg::cfx_agg_devicedriver::{build_agg_path_into, RendererScanlineAaOffset};
use crate::fxge::cfx_cttgsubtable::CfxCttgsubTable;
use crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap;
use crate::fxge::cfx_path::{CfxPath, PointType};
use crate::fxge::cfx_substfont::CfxSubstFont;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::FxdibFormat;
use crate::fxge::fx_font::{
    normalize_font_metric, CharCodeAndIndex, FontAntiAliasingMode, ADOBE_ENCODING_CUSTOM,
    ADOBE_ENCODING_EXPERT, ADOBE_ENCODING_LATIN1, ADOBE_ENCODING_STANDARD,
    APPLE_UNICODE_ENCODING_UNICODE_2_0, FONT_STYLE_FIXED_PITCH, FONT_STYLE_FORCE_BOLD,
    FONT_STYLE_ITALIC, FONT_STYLE_SERIF, FONT_STYLE_SYMBOLIC, MAC_ENCODING_ROMAN, PLATFORM_ADOBE,
    PLATFORM_APPLE_UNICODE, PLATFORM_ISO, PLATFORM_MAC, PLATFORM_WINDOWS, WINDOWS_ENCODING_BIG5,
    WINDOWS_ENCODING_GB2312, WINDOWS_ENCODING_JOHAB, WINDOWS_ENCODING_SJIS,
    WINDOWS_ENCODING_SYMBOL, WINDOWS_ENCODING_UCS4, WINDOWS_ENCODING_UNICODE,
    WINDOWS_ENCODING_WANSUNG,
};
use crate::fxge::fx_fontencoding::FontEncoding;
use crate::fxge::raw_postscript_face::RawPostScriptFace;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharMapIdPair {
    pub platform_id: u16,
    pub encoding_id: u16,
}

pub fn charmap_id_pair_to_font_encoding(pair: CharMapIdPair) -> FontEncoding {
    match (pair.platform_id, pair.encoding_id) {
        (PLATFORM_APPLE_UNICODE | PLATFORM_ISO, _) => FontEncoding::Unicode,
        (PLATFORM_MAC, MAC_ENCODING_ROMAN) => FontEncoding::AppleRoman,
        (PLATFORM_WINDOWS, WINDOWS_ENCODING_SYMBOL) => FontEncoding::Symbol,
        (PLATFORM_WINDOWS, WINDOWS_ENCODING_UNICODE | WINDOWS_ENCODING_UCS4) => {
            FontEncoding::Unicode
        }
        (PLATFORM_WINDOWS, WINDOWS_ENCODING_SJIS) => FontEncoding::Sjis,
        (PLATFORM_WINDOWS, WINDOWS_ENCODING_GB2312) => FontEncoding::GB2312,
        (PLATFORM_WINDOWS, WINDOWS_ENCODING_BIG5) => FontEncoding::Big5,
        (PLATFORM_WINDOWS, WINDOWS_ENCODING_WANSUNG) => FontEncoding::Wansung,
        (PLATFORM_WINDOWS, WINDOWS_ENCODING_JOHAB) => FontEncoding::Johab,
        (PLATFORM_ADOBE, ADOBE_ENCODING_STANDARD) => FontEncoding::AdobeStandard,
        (PLATFORM_ADOBE, ADOBE_ENCODING_EXPERT) => FontEncoding::AdobeExpert,
        (PLATFORM_ADOBE, ADOBE_ENCODING_CUSTOM) => FontEncoding::AdobeCustom,
        (PLATFORM_ADOBE, ADOBE_ENCODING_LATIN1) => FontEncoding::Latin1,
        _ => FontEncoding::None,
    }
}

// Fontations parses format 2 but does not map its character codes yet. Mirror
// FreeType's tt_cmap2_get_subheader()/tt_cmap2_char_index() using bounded reads.
// A byte that selects a nonzero subheader is a lead byte, not a single-byte
// character. Conversely, a two-byte code cannot use subheader zero.
fn cmap2_char_to_gid(cmap: &Cmap2<'_>, code: u32) -> Option<GlyphId> {
    let code = u16::try_from(code).ok()?;
    let low = code & 0xff;
    let high = code >> 8;
    let key = cmap
        .sub_header_keys()
        .get(usize::from(if high == 0 { low } else { high }))?
        .get();
    let sub_offset = if high == 0 {
        if key != 0 {
            return None;
        }
        518
    } else {
        let offset = usize::from(key & !7);
        if offset == 0 {
            return None;
        }
        518 + offset
    };
    let data = cmap.offset_data().slice(..usize::from(cmap.length()))?;
    let sub = data.read_ref_at::<SubHeader>(sub_offset).ok()?;
    let index = low.checked_sub(sub.first_code())?;
    if index >= sub.entry_count() || sub.id_range_offset() == 0 {
        return None;
    }
    let offset = sub_offset + 6 + usize::from(sub.id_range_offset()) + 2 * usize::from(index);
    let gid = data.read_at::<u16>(offset).ok()?;
    // An absent glyph remains absent even when idDelta is nonzero.
    (gid != 0).then(|| GlyphId::new(u32::from(gid.wrapping_add(sub.id_delta() as u16))))
}

// Same five-tap filter as CFX_Face::RenderGlyph. Gather each output once,
// retaining the original rounding of each tap before addition. The largest
// rounded contributions are 8 + 77 + 85 + 77 + 8 = 255, so a u16 sum exactly
// matches the original saturating byte additions.
fn filter_lcd_row(input: &[u8], output: &mut [u8]) {
    assert_eq!(input.len(), output.len());
    let width = input.len();
    let edge = |x: usize| -> u8 {
        [8u16, 77, 86, 77, 8]
            .into_iter()
            .enumerate()
            .filter_map(|(tap, weight)| {
                let source = x.checked_add(tap)?.checked_sub(2)?;
                input
                    .get(source)
                    .map(|&value| (u16::from(value) * weight + 85) >> 8)
            })
            .sum::<u16>() as u8
    };
    if width < 5 {
        for (x, value) in output.iter_mut().enumerate() {
            *value = edge(x);
        }
        return;
    }
    output[0] = edge(0);
    output[1] = edge(1);
    for (samples, value) in input.windows(5).zip(&mut output[2..width - 2]) {
        let tap = |index: usize, weight: u16| (u16::from(samples[index]) * weight + 85) >> 8;
        *value = (tap(0, 8) + tap(1, 77) + tap(2, 86) + tap(3, 77) + tap(4, 8)) as u8;
    }
    output[width - 2] = edge(width - 2);
    output[width - 1] = edge(width - 1);
}

// Mirrors the Fontations BoundsPen: include control points, even when the
// Bezier curve does not pass through them.
struct BoundsPen {
    x_min: f32,
    y_min: f32,
    x_max: f32,
    y_max: f32,
    has_points: bool,
}

impl BoundsPen {
    fn new() -> Self {
        Self {
            x_min: f32::MAX,
            y_min: f32::MAX,
            x_max: f32::MIN,
            y_max: f32::MIN,
            has_points: false,
        }
    }

    fn update(&mut self, x: f32, y: f32) {
        self.x_min = self.x_min.min(x);
        self.y_min = self.y_min.min(y);
        self.x_max = self.x_max.max(x);
        self.y_max = self.y_max.max(y);
        self.has_points = true;
    }
}

impl OutlinePen for BoundsPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.update(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.update(x, y);
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.update(cx, cy);
        self.update(x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.update(cx0, cy0);
        self.update(cx1, cy1);
        self.update(x, y);
    }

    fn close(&mut self) {}
}

struct GlyphPathPen {
    path: CfxPath,
    current_point: CfxPointF,
}

impl GlyphPathPen {
    fn new() -> Self {
        Self {
            path: CfxPath::new(),
            current_point: CfxPointF::new(0.0, 0.0),
        }
    }

    fn reset(&mut self) {
        self.path.clear();
        self.current_point = CfxPointF::new(0.0, 0.0);
    }
}

// Only temporary geometry/coverage storage is retained. Each returned glyph
// bitmap still owns independent pixels; no outlines or raster results are cached.
struct GlyphRenderScratch {
    pen: GlyphPathPen,
    path: PathStorage,
    curve: Curve4,
    rasterizer: RasterizerScanlineAa,
    expand_input: Vec<PathEl>,
    scanline: ScanlineU8,
    lcd_row: Vec<u8>,
}

impl GlyphRenderScratch {
    fn new() -> Self {
        Self {
            pen: GlyphPathPen::new(),
            path: PathStorage::new(),
            curve: Curve4::new(),
            rasterizer: RasterizerScanlineAa::new(),
            expand_input: Vec::new(),
            scanline: ScanlineU8::new(),
            lcd_row: Vec::new(),
        }
    }
}

impl Default for GlyphRenderScratch {
    fn default() -> Self {
        Self::new()
    }
}
impl Scratch for GlyphRenderScratch {
    fn retained_bytes(&self) -> usize {
        self.pen.path.retained_bytes()
            + self.path.retained_bytes()
            + self.curve.retained_bytes()
            + self.rasterizer.retained_bytes()
            + self.expand_input.capacity() * std::mem::size_of::<PathEl>()
            + self.scanline.retained_bytes()
            + self.lcd_row.capacity()
    }
}

impl OutlinePen for GlyphPathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.current_point = CfxPointF::new(x, y);
        self.path.append_point(self.current_point, PointType::Move);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current_point = CfxPointF::new(x, y);
        self.path.append_point(self.current_point, PointType::Line);
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let first = CfxPointF::new(
            self.current_point.x + (cx - self.current_point.x) * 2.0 / 3.0,
            self.current_point.y + (cy - self.current_point.y) * 2.0 / 3.0,
        );
        self.current_point = CfxPointF::new(x, y);
        let second = CfxPointF::new(cx + (x - cx) / 3.0, cy + (y - cy) / 3.0);
        self.path.append_point(first, PointType::Bezier);
        self.path.append_point(second, PointType::Bezier);
        self.path
            .append_point(self.current_point, PointType::Bezier);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.current_point = CfxPointF::new(x, y);
        self.path
            .append_point(CfxPointF::new(cx0, cy0), PointType::Bezier);
        self.path
            .append_point(CfxPointF::new(cx1, cy1), PointType::Bezier);
        self.path
            .append_point(self.current_point, PointType::Bezier);
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

/// Expand in device space before filling, rather than drawing a separate stroke.
/// FreeType's strength is the total increase in stem width. Its outline also
/// shifts by half the strength toward +x/+y (Y-up); preserve that placement in
/// our Y-down device space. LCD's horizontal supersampling scales both the
/// expansion and the shift, independently of the vertical component.
fn expand_glyph_outline(
    pen: &mut GlyphPathPen,
    device: CfxMatrix,
    level: i32,
    x_scale: f32,
    input: &mut Vec<PathEl>,
) -> Option<()> {
    input.clear();
    let transform = |point: CfxPointF| {
        let point = device.transform(point);
        (point.x.is_finite() && point.y.is_finite())
            .then(|| Point::new(f64::from(point.x), f64::from(point.y)))
    };
    let points = pen.path.points();
    let mut index = 0;
    while let Some(point) = points.get(index) {
        let end = match point.point_type {
            PointType::Move => {
                input.push(PathEl::MoveTo(transform(point.point)?));
                index
            }
            PointType::Line => {
                input.push(PathEl::LineTo(transform(point.point)?));
                index
            }
            PointType::Bezier => {
                let second = points.get(index + 1)?;
                let third = points.get(index + 2)?;
                input.push(PathEl::CurveTo(
                    transform(point.point)?,
                    transform(second.point)?,
                    transform(third.point)?,
                ));
                index + 2
            }
        };
        if points[end].close_figure {
            input.push(PathEl::ClosePath);
        }
        index = end + 1;
    }
    let radius = f64::from(level) / 128.0;
    let radius_x = radius * f64::from(x_scale);
    // The returned BezPath is a working allocation, dropped after conversion.
    // Only input's reusable capacity is retained and charged to scratch.
    let expanded = expand_path(
        input.as_slice(),
        Diagonal2::new(radius_x, radius),
        Join::Miter,
        2.0,
        0.01,
    );
    let shift = |point: Point| {
        let x = (point.x + radius_x) as f32;
        let y = (point.y - radius) as f32;
        (x.is_finite() && y.is_finite()).then_some((x, y))
    };
    pen.reset();
    for element in expanded.elements() {
        match *element {
            PathEl::MoveTo(p) => {
                let (x, y) = shift(p)?;
                pen.move_to(x, y);
            }
            PathEl::LineTo(p) => {
                let (x, y) = shift(p)?;
                pen.line_to(x, y);
            }
            PathEl::QuadTo(p, q) => {
                let (cx, cy) = shift(p)?;
                let (x, y) = shift(q)?;
                pen.quad_to(cx, cy, x, y);
            }
            PathEl::CurveTo(p, q, r) => {
                let (cx0, cy0) = shift(p)?;
                let (cx1, cy1) = shift(q)?;
                let (x, y) = shift(r)?;
                pen.curve_to(cx0, cy0, cx1, cy1, x, y);
            }
            PathEl::ClosePath => pen.close(),
        }
    }
    Some(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CfxFaceError {
    InvalidFont,
}

/// FreeType sfnt_load_face()'s Unicode-cmap synthesis, using Fontations' AGL
/// mapper. An existing symbol cmap also suppresses synthesis (its PUA mapping
/// is intentional). Empty/unmappable post names must preserve the no-map case.
fn synthesize_sfnt_unicode(font: &FontRef<'_>) -> Option<UnicodeCharmap> {
    if let Ok(cmap) = font.cmap() {
        if cmap.encoding_records().iter().any(|record| {
            let encoding = charmap_id_pair_to_font_encoding(CharMapIdPair {
                platform_id: record.platform_id() as u16,
                encoding_id: record.encoding_id(),
            });
            matches!(encoding, FontEncoding::Unicode | FontEncoding::Symbol)
                && record.subtable(cmap.offset_data()).is_ok()
        }) {
            return None;
        }
    }
    let post = font.post().ok()?;
    let count = font.maxp().ok()?.num_glyphs() as u32;
    // Use read-fonts' actual glyph IDs. Skrifa's name iterator can renumber
    // entries when an unreadable name is skipped in a damaged post table.
    let map = UnicodeCharmap::from_glyph_names(
        post.glyph_names()
            .take_while(|(gid, _)| gid.to_u32() < count),
    );
    let has_mapping = map.iter().next().is_some();
    has_mapping.then_some(map)
}

#[derive(yoke::Yokeable)]
enum FaceSource<'a> {
    // Match the pinned Fontations adapter's Sfnt: keep outline metadata and
    // the expensive tricky-font detection for the lifetime of the font bytes.
    Sfnt {
        font: FontRef<'a>,
        outlines: OutlineGlyphCollection<'a>,
        is_tricky: bool,
        synthesized_unicode: Option<UnicodeCharmap>,
    },
    // CEF fonts can wrap a complete CFF in an OTTO directory with no head,
    // maxp, or metric tables. Keep its optional cmap; use CFF metrics/outlines.
    CffSfnt {
        font: FontRef<'a>,
        raw: RawPostScriptFace<'a>,
        synthesize_unicode: bool,
    },
    PostScript(RawPostScriptFace<'a>),
}

#[derive(yoke::Yokeable)]
pub struct CfxFace<'a> {
    data: &'a [u8],
    cache_id: u64,
    face_index: u32,
    source: FaceSource<'a>,
    // The outer option caches the attempt, including a failed initialization.
    hinter: RefCell<Option<Option<HintingInstance>>>,
    render_scratch: Rc<ScratchResource<GlyphRenderScratch>>,
    selected_charmap_index: Cell<Option<usize>>,
    selected_encoding: Cell<FontEncoding>,
}

/// A parsed face tied to shared font bytes without copying or reparsing them.
/// `Yoke` keeps the byte owner alive until after the borrowed face is dropped.
pub struct CfxOwnedFace<C: std::ops::Deref>(yoke::Yoke<CfxFace<'static>, C>);

/// A parsed face with a retained byte owner, independent of its cart type.
pub trait CfxRetainedFace {
    fn face(&self) -> &CfxFace<'_>;
}

impl<C> CfxRetainedFace for CfxOwnedFace<C>
where
    C: std::ops::Deref + stable_deref_trait::StableDeref,
    C::Target: AsRef<[u8]> + 'static,
{
    fn face(&self) -> &CfxFace<'_> {
        self.0.get()
    }
}

impl<C> CfxOwnedFace<C>
where
    C: std::ops::Deref + stable_deref_trait::StableDeref,
    C::Target: AsRef<[u8]> + 'static,
{
    pub fn new(data: C, face_index: u32) -> Result<Self, CfxFaceError> {
        Self::new_with_memory(data, face_index, super::font_memory::font_memory())
    }

    pub fn new_with_memory(
        data: C,
        face_index: u32,
        memory: Rc<FontMemory>,
    ) -> Result<Self, CfxFaceError> {
        yoke::Yoke::try_attach_to_cart(data, move |bytes| {
            CfxFace::new_with_memory(bytes.as_ref(), face_index, memory)
        })
        .map(Self)
    }

    pub fn face(&self) -> &CfxFace<'_> {
        self.0.get()
    }

    pub fn data_owner(&self) -> &C {
        self.0.backing_cart()
    }

    pub fn with_face_mut<R: 'static>(
        &mut self,
        f: impl FnOnce(&mut CfxFace<'_>) -> R + 'static,
    ) -> R {
        self.0.with_mut_return(f)
    }
}

impl<'a> CfxFace<'a> {
    fn next_cache_id() -> u64 {
        static NEXT_FACE_ID: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT_FACE_ID.fetch_add(1, Ordering::Relaxed);
        assert_ne!(identity, 0, "face identities exhausted");
        identity
    }

    pub(crate) fn cache_id(&self) -> u64 {
        self.cache_id
    }

    pub fn unicode_from_adobe_name(name: &str) -> u32 {
        let name = name.split('\0').next().unwrap_or_default();
        read_fonts::ps::agl::name_to_char(name).map_or(0, |ch| ch as u32)
    }

    pub fn adobe_name_from_unicode(unicode: u32) -> Vec<u8> {
        let mut buffer = [0; 64];
        read_fonts::ps::agl::char_to_name(unicode, &mut buffer)
            .map_or_else(Vec::new, |name| name.as_bytes().to_vec())
    }

    pub fn new(data: &'a [u8], face_index: u32) -> Result<Self, CfxFaceError> {
        Self::new_with_memory(data, face_index, super::font_memory::font_memory())
    }

    pub fn new_with_memory(
        data: &'a [u8],
        face_index: u32,
        memory: Rc<FontMemory>,
    ) -> Result<Self, CfxFaceError> {
        if let Ok(font) = FontRef::from_index(data, face_index) {
            // FreeType cff_face_init() treats an OTTO wrapper without head as
            // pure CFF, loading only its optional cmap from the SFNT directory.
            if font.table_directory().sfnt_version() == u32::from_be_bytes(*b"OTTO")
                && font.table_data(Tag::new(b"head")).is_none()
            {
                let cff = font
                    .table_data(Tag::new(b"CFF "))
                    .ok_or(CfxFaceError::InvalidFont)?;
                // The outer collection selects the face; the CFF table itself
                // contains one font, so do not reuse the collection index.
                let raw = RawPostScriptFace::from_cff(cff.as_bytes(), 0)
                    .ok_or(CfxFaceError::InvalidFont)?;
                let has_unicode = font.cmap().is_ok_and(|cmap| {
                    cmap.encoding_records().iter().any(|record| {
                        charmap_id_pair_to_font_encoding(CharMapIdPair {
                            platform_id: record.platform_id() as u16,
                            encoding_id: record.encoding_id(),
                        }) == FontEncoding::Unicode
                            && record.subtable(cmap.offset_data()).is_ok()
                    })
                });
                return Ok(Self {
                    data,
                    cache_id: Self::next_cache_id(),
                    face_index,
                    source: FaceSource::CffSfnt {
                        font,
                        raw,
                        synthesize_unicode: !has_unicode,
                    },
                    hinter: RefCell::new(None),
                    render_scratch: ScratchResource::new(memory),
                    selected_charmap_index: Cell::new(None),
                    selected_encoding: Cell::new(FontEncoding::None),
                });
            }
            // sfnt_load_face() rejects invalid em sizes before making a face.
            // Accepting one here prevents the PDF loader's normal substitution
            // and leaves a retained face whose glyphs cannot be scaled.
            let units_per_em = font
                .head()
                .map_err(|_| CfxFaceError::InvalidFont)?
                .units_per_em();
            if !(16..=16384).contains(&units_per_em) {
                return Err(CfxFaceError::InvalidFont);
            }
            let outlines = font.outline_glyphs();
            // Reading an SFNT directory can succeed even when required outline
            // tables are missing/truncated. Such a face cannot render any text.
            // Reject it at load time, as native FreeType does, so the PDF font
            // loader purges the failed embedded font and selects a substitute.
            if outlines.format().is_none() {
                return Err(CfxFaceError::InvalidFont);
            }
            let is_tricky = outlines.require_interpreter();
            // sfnt_load_face() synthesizes a Unicode charmap from post names
            // when neither a Unicode nor a symbol charmap exists. Without it,
            // PDF simple fonts can incorrectly take the sequential-GID path.
            let synthesized_unicode = synthesize_sfnt_unicode(&font);
            return Ok(Self {
                data,
                cache_id: Self::next_cache_id(),
                face_index,
                source: FaceSource::Sfnt {
                    font,
                    outlines,
                    is_tricky,
                    synthesized_unicode,
                },
                hinter: RefCell::new(None),
                render_scratch: ScratchResource::new(memory),
                selected_charmap_index: Cell::new(None),
                selected_encoding: Cell::new(FontEncoding::None),
            });
        }
        let raw = RawPostScriptFace::from_cff(data, face_index)
            .or_else(|| RawPostScriptFace::from_type1(data))
            .or_else(|| RawPostScriptFace::from_cid_type1(data))
            .ok_or(CfxFaceError::InvalidFont)?;
        Ok(Self {
            data,
            cache_id: Self::next_cache_id(),
            face_index,
            source: FaceSource::PostScript(raw),
            hinter: RefCell::new(None),
            render_scratch: ScratchResource::new(memory),
            selected_charmap_index: Cell::new(None),
            selected_encoding: Cell::new(FontEncoding::None),
        })
    }

    fn sfnt(&self) -> &FontRef<'a> {
        self.sfnt_opt().expect("face is not SFNT")
    }

    fn sfnt_opt(&self) -> Option<&FontRef<'a>> {
        match &self.source {
            FaceSource::Sfnt { font, .. } | FaceSource::CffSfnt { font, .. } => Some(font),
            FaceSource::PostScript(_) => None,
        }
    }

    fn outlines(&self) -> &OutlineGlyphCollection<'a> {
        match &self.source {
            FaceSource::Sfnt { outlines, .. } => outlines,
            FaceSource::PostScript(_) | FaceSource::CffSfnt { .. } => {
                panic!("face has raw PostScript outlines")
            }
        }
    }

    fn raw(&self) -> Option<&RawPostScriptFace<'a>> {
        match &self.source {
            FaceSource::PostScript(raw) | FaceSource::CffSfnt { raw, .. } => Some(raw),
            FaceSource::Sfnt { .. } => None,
        }
    }

    fn synthesized_unicode(&self) -> Option<&UnicodeCharmap> {
        match &self.source {
            FaceSource::Sfnt {
                synthesized_unicode,
                ..
            } => synthesized_unicode.as_ref(),
            FaceSource::CffSfnt {
                raw,
                synthesize_unicode: true,
                ..
            } => raw.synthesized_unicode(),
            FaceSource::CffSfnt { .. } => None,
            FaceSource::PostScript(raw) => raw.synthesized_unicode(),
        }
    }

    fn sfnt_charmap_count(&self) -> usize {
        self.sfnt()
            .cmap()
            .ok()
            .map_or(0, |cmap| cmap.encoding_records().len())
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn face_index(&self) -> u32 {
        self.face_index
    }

    pub fn is_tt_ot(&self) -> bool {
        self.sfnt_opt().is_some()
    }

    pub fn font_format(&self) -> &'static str {
        if let Some(raw) = self.raw() {
            return if raw.is_cff() { "CFF" } else { "Type 1" };
        }
        match self.outlines().format() {
            Some(OutlineGlyphFormat::Glyf) => "TrueType",
            Some(OutlineGlyphFormat::Cff | OutlineGlyphFormat::Cff2) => "CFF",
            _ => "",
        }
    }

    fn is_postscript_font(&self) -> bool {
        !matches!(self.source, FaceSource::CffSfnt { .. })
            && matches!(self.font_format(), "CFF" | "Type 1")
    }

    pub fn has_glyph_names(&self) -> bool {
        if let Some(raw) = self.raw() {
            return raw.has_glyph_names();
        }
        self.sfnt().glyph_names().source() != GlyphNameSource::Synthesized
    }

    pub fn family_name(&self) -> String {
        if let Some(raw) = self.raw() {
            return raw.family_name().to_owned();
        }
        self.sfnt()
            .localized_strings(StringId::TYPOGRAPHIC_FAMILY_NAME)
            .english_or_first()
            .or_else(|| {
                self.sfnt()
                    .localized_strings(StringId::FAMILY_NAME)
                    .english_or_first()
            })
            .map_or_else(String::new, |name| name.to_string())
    }

    pub fn style_name(&self) -> String {
        if self.raw().is_some() {
            return String::new();
        }
        let mut name = self
            .sfnt()
            .localized_strings(StringId::SUBFAMILY_NAME)
            .english_or_first()
            .map_or_else(String::new, |name| name.to_string());
        // Native wraps the Fontations style_name Rust String via c_str(),
        // whereas family_name uses a length-bearing view.
        if let Some(end) = name.find('\0') {
            name.truncate(end);
        }
        name
    }

    pub fn postscript_name(&self) -> String {
        if let Some(raw) = self.raw() {
            return raw.postscript_name().to_owned();
        }
        self.sfnt()
            .localized_strings(StringId::POSTSCRIPT_NAME)
            .english_or_first()
            .map_or_else(String::new, |name| name.to_string())
    }

    /// Includes PostScript FontMatrix scales, which can exceed SFNT's u16 range.
    pub fn units_per_em(&self) -> u32 {
        if let Some(raw) = self.raw() {
            return raw.upem();
        }
        u32::from(
            self.sfnt()
                .metrics(Size::unscaled(), LocationRef::default())
                .units_per_em,
        )
    }

    pub fn ascender(&self) -> i16 {
        if let Some(raw) = self.raw() {
            return raw.bbox().map_or(0.0, |bbox| bbox.3).round() as i16;
        }
        self.sfnt()
            .metrics(Size::unscaled(), LocationRef::default())
            .ascent
            .round() as i16
    }

    pub fn descender(&self) -> i16 {
        if let Some(raw) = self.raw() {
            return raw.bbox().map_or(0.0, |bbox| bbox.1).round() as i16;
        }
        self.sfnt()
            .metrics(Size::unscaled(), LocationRef::default())
            .descent
            .round() as i16
    }

    pub fn glyph_count(&self) -> u32 {
        if let Some(raw) = self.raw() {
            return raw.glyph_count();
        }
        self.sfnt()
            .metrics(Size::unscaled(), LocationRef::default())
            .glyph_count as u32
    }

    pub fn glyph_tt_width(&self, glyph_index: u32) -> i32 {
        self.unscaled_glyph_advance(glyph_index).map_or(0, |width| {
            normalize_font_metric((width + 0.5) as i64, self.units_per_em())
        })
    }

    fn unscaled_glyph_advance(&self, glyph_index: u32) -> Option<f32> {
        self.draw_unscaled(glyph_index, &mut BoundsPen::new())
            .map(Option::unwrap_or_default)
    }

    pub fn glyph_width(
        &self,
        glyph_index: u32,
        dest_width: i32,
        subst: Option<&CfxSubstFont>,
    ) -> i32 {
        self.glyph_width_with_weight(glyph_index, dest_width, 0, subst)
    }

    pub fn glyph_width_with_weight(
        &self,
        glyph_index: u32,
        dest_width: i32,
        weight: i32,
        subst: Option<&CfxSubstFont>,
    ) -> i32 {
        if self.uses_multiple_master(subst) {
            return self
                .raw()
                .and_then(|raw| raw.width_with_variation(glyph_index, dest_width, weight))
                .unwrap_or(0);
        }
        if subst.is_some_and(CfxSubstFont::is_builtin_generic_font) && dest_width > 0 {
            return dest_width;
        }
        let Some(advance) = self.unscaled_glyph_advance(glyph_index) else {
            return 0;
        };
        let advance = advance as i32;
        let upem = self.units_per_em();
        if upem == 0 {
            advance
        } else {
            (i64::from(advance) * 1000 / i64::from(upem)) as i32
        }
    }

    pub fn is_tricky(&self) -> bool {
        match &self.source {
            FaceSource::Sfnt { is_tricky, .. } => *is_tricky,
            FaceSource::PostScript(_) | FaceSource::CffSfnt { .. } => false,
        }
    }

    pub fn char_bbox(&self, glyph_index: i32) -> FxRect {
        if glyph_index < 0 || !self.has_outline(glyph_index as u32) {
            return FxRect::default();
        }
        let mut rect = self.glyph_bbox(glyph_index as u32);
        if self.is_tricky() {
            rect.top = rect.top.min(i32::from(self.ascender()));
            rect.bottom = rect.bottom.max(i32::from(self.descender()));
        } else {
            rect.top = rect.top.saturating_add(rect.top / 64);
        }
        rect
    }

    fn has_outline(&self, glyph_index: u32) -> bool {
        if let Some(raw) = self.raw() {
            raw.has_outline(glyph_index)
        } else {
            self.outlines().get(GlyphId::new(glyph_index)).is_some()
        }
    }

    fn uses_multiple_master(&self, subst: Option<&CfxSubstFont>) -> bool {
        subst.is_some_and(CfxSubstFont::is_builtin_generic_font)
            && self.raw().is_some_and(|raw| raw.has_multiple_master())
    }

    fn draw_unscaled(&self, glyph_index: u32, pen: &mut impl OutlinePen) -> Option<Option<f32>> {
        if let Some(raw) = self.raw() {
            return raw.draw(glyph_index, pen);
        }
        let gid = GlyphId::new(glyph_index);
        let glyph = self.outlines().get(gid)?;
        let metrics = glyph.draw(Size::unscaled(), pen).ok()?;
        Some(metrics.advance_width.or_else(|| {
            self.sfnt()
                .glyph_metrics(Size::unscaled(), LocationRef::default())
                .advance_width(gid)
        }))
    }

    /// Drawing policy shared by bitmap masks and extracted paths: ordinary
    /// outlines stay unhinted; hint-reliant fonts need the embedded interpreter
    /// to assemble their contours. The fixed mono target controls that program,
    /// independently of the eventual AGG antialiasing mode or render size.
    /// Returns the advance in font units and the outline units per em.
    /// Keep the denominator: multiplying by a rounded reciprocal can push
    /// an exact glyph edge across an integer device boundary.
    fn draw_outline(
        &self,
        glyph_index: u32,
        dest_width: i32,
        subst: Option<&CfxSubstFont>,
        pen: &mut impl OutlinePen,
    ) -> Option<(Option<f32>, f32)> {
        let upem = self.units_per_em() as f32;
        if upem == 0.0 {
            return None;
        }
        if self.uses_multiple_master(subst) {
            let advance =
                self.raw()?
                    .draw_with_variation(glyph_index, dest_width, subst?.weight(), pen)?;
            return Some((advance, upem));
        }
        if !self.is_tricky() {
            return Some((self.draw_unscaled(glyph_index, pen)?, upem));
        }
        const HINT_RELIANT_PPEM: f32 = 1000.0;
        let glyph_id = GlyphId::new(glyph_index);
        let outlines = self.outlines();
        let glyph = outlines.get(glyph_id)?;
        let mut hinter = self.hinter.borrow_mut();
        if hinter.is_none() {
            #[cfg(feature = "render-profile")]
            if crate::render_profile::trace_glyphs() {
                eprintln!(
                    "glyph-hinter face={} name={:?}",
                    self.cache_id,
                    self.postscript_name()
                );
            }
            *hinter = Some(
                HintingInstance::new(
                    outlines,
                    Size::new(HINT_RELIANT_PPEM),
                    LocationRef::default(),
                    HintingOptions {
                        engine: Engine::Interpreter,
                        target: Target::Mono,
                    },
                )
                .ok(),
            );
        }
        let instance = hinter.as_ref().and_then(Option::as_ref)?;
        let metrics = glyph.draw(DrawSettings::hinted(instance, false), pen).ok();
        #[cfg(feature = "render-profile")]
        if crate::render_profile::trace_glyphs() {
            eprintln!(
                "glyph-hint face={} glyph={} ok={} name={:?}",
                self.cache_id,
                glyph_index,
                metrics.is_some(),
                self.postscript_name(),
            );
        }
        // Unhinted fallback would return malformed contours for these fonts.
        let advance = metrics?.advance_width.or_else(|| {
            self.sfnt()
                .glyph_metrics(Size::new(HINT_RELIANT_PPEM), LocationRef::default())
                .advance_width(glyph_id)
        });
        Some((
            advance.map(|value| value * upem / HINT_RELIANT_PPEM),
            HINT_RELIANT_PPEM,
        ))
    }

    pub fn load_glyph_path(
        &self,
        glyph_index: u32,
        dest_width: i32,
        is_vertical: bool,
        subst: Option<&CfxSubstFont>,
    ) -> Option<CfxPath> {
        let mut pen = GlyphPathPen::new();
        let (advance, outline_units) =
            self.draw_outline(glyph_index, dest_width, subst, &mut pen)?;
        let upem = self.units_per_em();
        if upem == 0 || pen.path.points().is_empty() {
            return None;
        }
        // Divide the original coordinates before applying font substitutions.
        // For example, 700 / 1000 rounds to the nearest f32 representation
        // of 0.7; 700 * (1 / 1000) instead rounds above it and makes a
        // 100pt rectangle acquire an extra solid row in DrawPath().
        for point in pen.path.points_mut() {
            point.point.x /= outline_units;
            point.point.y /= outline_units;
        }
        let mut matrix = CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        if subst.is_some_and(CfxSubstFont::is_builtin_generic_font)
            && !self.uses_multiple_master(subst)
            && dest_width > 0
        {
            let advance = advance.unwrap_or_default();
            if advance > 0.0 {
                let glyph_width = (i64::from(advance as i32) * 1000 / i64::from(upem)) as i32;
                if glyph_width > 0 {
                    let scale_x = dest_width as f32 / glyph_width as f32;
                    matrix.a *= scale_x;
                    matrix.b *= scale_x;
                }
            }
        }
        let skew = subst.map_or(0, |font| font.effective_skew(false));
        if skew != 0 {
            if is_vertical {
                matrix.b += matrix.d * skew as f32 / 100.0;
            } else {
                matrix.c -= matrix.a * skew as f32 / 100.0;
            }
        }
        pen.path.transform(matrix);
        let level = subst.map_or(0, CfxSubstFont::embolden_level_for_load);
        if level > 0 {
            // Native LoadGlyphPath applies substitution skew at 64 ppem, then
            // FT_Outline_Embolden in 26.6 units before returning a font-em path.
            // Reuse the bitmap path's safe contour expansion in its Y-down
            // space; the second transform restores normalized Y-up geometry.
            let mut input = Vec::new();
            expand_glyph_outline(
                &mut pen,
                CfxMatrix::new(64.0, 0.0, 0.0, -64.0, 0.0, 0.0),
                level,
                1.0,
                &mut input,
            )?;
            pen.path
                .transform(CfxMatrix::new(1.0 / 64.0, 0.0, 0.0, -1.0 / 64.0, 0.0, 0.0));
        }
        Some(pen.path)
    }

    /// Render a Fontations outline through the shared AGG scanline rasterizer.
    /// PDFium uses FreeType's outline rasterizer here; the resulting coverage
    /// can differ while preserving its glyph transform and bitmap placement.
    pub fn render_glyph(
        &self,
        glyph_index: u32,
        is_cid_font: bool,
        is_vertical: bool,
        matrix: CfxMatrix,
        dest_width: i32,
        anti_alias: FontAntiAliasingMode,
        subst: Option<&CfxSubstFont>,
    ) -> Option<CfxGlyphBitmap> {
        // The only outline callbacks while borrowed go to our private pen,
        // which only appends points and cannot reenter font rendering.
        let mut scratch = self.render_scratch.checkout();
        let GlyphRenderScratch {
            pen,
            path,
            curve,
            rasterizer,
            expand_input,
            scanline,
            lcd_row,
        } = &mut *scratch;
        pen.reset();
        let (advance, outline_units) = self.draw_outline(glyph_index, dest_width, subst, pen)?;
        let upem = self.units_per_em();
        if pen.path.points().is_empty() {
            return None;
        }

        // CFX_Face::AdjustSubstFontTransform() works in glyph space, before
        // PDFium converts outline coordinates to pixels.
        let mut effective = matrix;
        if let Some(subst) = subst {
            if subst.is_builtin_generic_font()
                && !self.uses_multiple_master(Some(subst))
                && dest_width > 0
            {
                if let Some(advance) = advance.filter(|&advance| advance > 0.0) {
                    let glyph_width = (i64::from(advance as i32) * 1000 / i64::from(upem)) as i32;
                    if glyph_width > 0 {
                        let scale_x = dest_width as f32 / glyph_width as f32;
                        effective.a *= scale_x;
                        effective.b *= scale_x;
                    }
                }
            }
            let skew = subst.effective_skew(is_cid_font);
            if is_vertical {
                effective.b += effective.d * skew as f32 / 100.0;
            } else {
                effective.c -= effective.a * skew as f32 / 100.0;
            }
        }

        // The outline is in a Y-up font space. AGG and CFX_DIBitmap use
        // top-down scanlines. PDFium excludes matrix translation here.
        let x_scale = if anti_alias == FontAntiAliasingMode::Lcd {
            3.0
        } else {
            1.0
        };
        let device = CfxMatrix::new(
            (effective.a / outline_units) * x_scale,
            -effective.b / outline_units,
            (effective.c / outline_units) * x_scale,
            -effective.d / outline_units,
            0.0,
            0.0,
        );
        let mut outline_to_device = Some(device);
        if let Some(subst) = subst {
            let matrix_xx = (matrix.a * 1024.0) as i32;
            let mut matrix_xy = (matrix.c * 1024.0) as i32;
            let skew = subst.effective_skew(is_cid_font);
            if skew != 0 && !is_vertical {
                matrix_xy = matrix_xy.saturating_sub(matrix_xx.saturating_mul(skew) / 100);
            }
            let level = subst.embolden_level_for_render(is_cid_font, matrix_xx, matrix_xy);
            if level < 0 {
                return None;
            }
            if level > 0 {
                expand_glyph_outline(pen, device, level, x_scale, expand_input)?;
                outline_to_device = None;
            }
        }
        build_agg_path_into(&pen.path, outline_to_device, path, curve);
        rasterizer.reset();
        rasterizer.add_path_truncated(path, 0);
        if !rasterizer.sort() {
            return None;
        }
        let (min_x, min_y, max_x, max_y) = (
            rasterizer.min_x(),
            rasterizer.min_y(),
            rasterizer.max_x(),
            rasterizer.max_y(),
        );
        let (left, width) = if anti_alias == FontAntiAliasingMode::Lcd {
            // Two subpixels of padding on each side for the five-tap filter.
            let left = min_x.checked_sub(2)?.div_euclid(3);
            let right = max_x.checked_add(5)?.div_euclid(3);
            (left, right.checked_sub(left)?.checked_mul(3)?)
        } else {
            (min_x, max_x.checked_sub(min_x)?.checked_add(1)?)
        };
        let height = max_y.checked_sub(min_y)?.checked_add(1)?;
        if width <= 0 || height <= 0 || width > 2048 || height > 2048 {
            return None;
        }
        let mut gray = CfxDibitmap::create(width, height, FxdibFormat::EightBppMask, 0)?;
        let stride = i32::try_from(gray.pitch()).ok()?;
        {
            let mut buffer = RenderingBuffer::from_buffer(
                gray.buffer_mut(),
                width as u32,
                height as u32,
                stride,
            )?;
            let mut pixels = PixfmtGray8::new(&mut buffer);
            let mut base = RendererBaseGray::with_pixel_format(&mut pixels);
            let offset_x = if anti_alias == FontAntiAliasingMode::Lcd {
                left.checked_mul(3)?
            } else {
                left
            };
            let mut renderer = RendererScanlineAaOffset::new(&mut base, offset_x, min_y);
            renderer.set_color(Gray8::opaque(255));
            if !render_scanlines(
                rasterizer,
                scanline,
                &mut renderer,
                anti_alias == FontAntiAliasingMode::Mono,
            ) {
                return None;
            }
        }
        let bitmap = match anti_alias {
            FontAntiAliasingMode::Normal => gray,
            FontAntiAliasingMode::Mono => {
                let mut mono = CfxDibitmap::create(width, height, FxdibFormat::OneBppMask, 0)?;
                for y in 0..height {
                    let input = gray.scanline(y)?;
                    let output = mono.scanline_mut(y)?;
                    for x in 0..width as usize {
                        if input[x] != 0 {
                            output[x / 8] |= 0x80 >> (x % 8);
                        }
                    }
                }
                mono
            }
            FontAntiAliasingMode::Lcd => {
                lcd_row.resize(width as usize, 0);
                for y in 0..height {
                    let row = &mut gray.scanline_mut(y)?[..width as usize];
                    lcd_row.copy_from_slice(row);
                    filter_lcd_row(lcd_row, row);
                }
                gray
            }
        };
        Some(CfxGlyphBitmap::new(
            CfxPoint::new(left, min_y.checked_neg()?),
            Rc::new(RefCell::new(bitmap)),
        ))
    }

    fn unscaled_glyph_bounds(&self, glyph_index: u32) -> (f32, f32, f32, f32) {
        let mut pen = BoundsPen::new();
        if self.draw_unscaled(glyph_index, &mut pen).is_none() || !pen.has_points {
            return (0.0, 0.0, 0.0, 0.0);
        }
        (pen.x_min, pen.y_min, pen.x_max, pen.y_max)
    }

    fn normalized_glyph_bounds(&self, glyph_index: u32) -> FxRect {
        if self.is_tricky() {
            let mut pen = BoundsPen::new();
            // The interpreter can reconstruct contours outside their unhinted
            // bounds. Use the same fixed-mono outline as paths and bitmaps;
            // otherwise text culling and transparency buffers can clip them.
            let Some((_, outline_units)) = self.draw_outline(glyph_index, 0, None, &mut pen) else {
                return FxRect::default();
            };
            if !pen.has_points {
                return FxRect::default();
            }
            // Convert directly from the hinted outline to 1000-unit metrics,
            // avoiding a lossy integer round trip through font units. Round
            // outwards, like FreeType's tricky-font pixel bounding box.
            let scale = 1000.0 / outline_units;
            return FxRect::new(
                (pen.x_min * scale).floor() as i32,
                (pen.y_min * scale).floor() as i32,
                (pen.x_max * scale).ceil() as i32,
                (pen.y_max * scale).ceil() as i32,
            );
        }
        let (x_min, y_min, x_max, y_max) = self.unscaled_glyph_bounds(glyph_index);
        let upem = self.units_per_em();
        FxRect::new(
            normalize_font_metric(x_min as i64, upem),
            normalize_font_metric(y_min as i64, upem),
            normalize_font_metric(x_max as i64, upem),
            normalize_font_metric(y_max as i64, upem),
        )
    }

    pub fn glyph_bbox(&self, glyph_index: u32) -> FxRect {
        let bounds = self.normalized_glyph_bounds(glyph_index);
        FxRect::new(bounds.left, bounds.bottom, bounds.right, bounds.top)
    }

    pub fn font_glyph_bbox(&self, glyph_index: u32) -> Option<FxRect> {
        if !self.has_outline(glyph_index) {
            return None;
        }
        Some(self.normalized_glyph_bounds(glyph_index))
    }

    pub fn is_fixed_width(&self) -> bool {
        if let Some(raw) = self.raw() {
            return raw.is_fixed_width();
        }
        self.sfnt()
            .metrics(Size::unscaled(), LocationRef::default())
            .is_monospace
    }

    pub fn is_italic(&self) -> bool {
        if let Some(raw) = self.raw() {
            return raw.is_italic();
        }
        if let Ok(os2) = self.sfnt().os2() {
            let flags = os2.fs_selection();
            return flags.contains(SelectionFlags::ITALIC)
                || flags.contains(SelectionFlags::OBLIQUE);
        }
        self.sfnt()
            .head()
            .is_ok_and(|head| head.mac_style().contains(MacStyle::ITALIC))
    }

    pub fn is_bold(&self) -> bool {
        if let Some(raw) = self.raw() {
            return raw.is_bold();
        }
        if let Ok(os2) = self.sfnt().os2() {
            return os2.fs_selection().contains(SelectionFlags::BOLD);
        }
        self.sfnt()
            .head()
            .is_ok_and(|head| head.mac_style().contains(MacStyle::BOLD))
    }

    pub fn bbox(&self) -> FxRect {
        if let Some(raw) = self.raw() {
            return raw.bbox().map_or(
                FxRect::new(-1000, -1000, 1000, 1000),
                |(left, top, right, bottom)| {
                    FxRect::new(
                        left.round() as i32,
                        top.round() as i32,
                        right.round() as i32,
                        bottom.round() as i32,
                    )
                },
            );
        }
        let metrics = self
            .sfnt()
            .metrics(Size::unscaled(), LocationRef::default());
        if let Some(bounds) = metrics.bounds {
            return FxRect::new(
                bounds.x_min.round() as i32,
                bounds.y_min.round() as i32,
                bounds.x_max.round() as i32,
                bounds.y_max.round() as i32,
            );
        }
        if let Ok(head) = self.sfnt().head() {
            return FxRect::new(
                i32::from(head.x_min()),
                i32::from(head.y_min()),
                i32::from(head.x_max()),
                i32::from(head.y_max()),
            );
        }
        FxRect::new(-1000, -1000, 1000, 1000)
    }

    pub fn unicode_char_index(&self, code: u32) -> u32 {
        if let FaceSource::PostScript(raw) = &self.source {
            return raw.unicode_char_index(code);
        }
        if let Some(map) = self.synthesized_unicode() {
            return map.map(code).map_or(0, |glyph| glyph.to_u32());
        }
        let charmap = self.sfnt().charmap();
        if charmap.has_map() {
            return charmap.map(code).map_or(0, |glyph| glyph.to_u32());
        }
        self.unicode_cmap2()
            .and_then(|map| cmap2_char_to_gid(&map, code))
            .filter(|glyph| glyph.to_u32() < self.glyph_count())
            .map_or(0, |glyph| glyph.to_u32())
    }

    fn unicode_cmap2(&self) -> Option<Cmap2<'a>> {
        let cmap = self.sfnt().cmap().ok()?;
        cmap.encoding_records().iter().rev().find_map(|record| {
            let pair = CharMapIdPair {
                platform_id: record.platform_id() as u16,
                encoding_id: record.encoding_id(),
            };
            if charmap_id_pair_to_font_encoding(pair) != FontEncoding::Unicode {
                return None;
            }
            match record.subtable(cmap.offset_data()).ok()? {
                CmapSubtable::Format2(map) => Some(map),
                _ => None,
            }
        })
    }

    pub fn charmap_count(&self) -> usize {
        if self.raw().is_some_and(RawPostScriptFace::is_cid_type1) {
            return 0;
        }
        if self.is_postscript_font() {
            return if self.raw().is_some_and(RawPostScriptFace::has_encoding) {
                2
            } else {
                1
            };
        }
        self.sfnt_charmap_count()
            + usize::from(self.synthesized_unicode().is_some())
            + usize::from(
                matches!(&self.source, FaceSource::CffSfnt { raw, .. } if raw.has_glyph_names() && raw.has_encoding()),
            )
    }

    pub fn charmap_id_pair_by_index(&self, index: usize) -> CharMapIdPair {
        if self.is_postscript_font() {
            return if index == 0 {
                CharMapIdPair {
                    platform_id: PLATFORM_APPLE_UNICODE,
                    encoding_id: APPLE_UNICODE_ENCODING_UNICODE_2_0,
                }
            } else {
                CharMapIdPair {
                    platform_id: PLATFORM_ADOBE,
                    encoding_id: ADOBE_ENCODING_CUSTOM,
                }
            };
        }
        if index == self.sfnt_charmap_count() && self.synthesized_unicode().is_some() {
            return CharMapIdPair {
                platform_id: PLATFORM_WINDOWS,
                encoding_id: WINDOWS_ENCODING_UNICODE,
            };
        }
        if index == self.sfnt_charmap_count() + usize::from(self.synthesized_unicode().is_some())
            && matches!(&self.source, FaceSource::CffSfnt { raw, .. } if raw.has_glyph_names() && raw.has_encoding())
        {
            return CharMapIdPair {
                platform_id: PLATFORM_ADOBE,
                encoding_id: ADOBE_ENCODING_CUSTOM,
            };
        }
        let record = self
            .sfnt()
            .cmap()
            .ok()
            .and_then(|cmap| cmap.encoding_records().get(index).copied());
        record.map_or(
            CharMapIdPair {
                platform_id: 0,
                encoding_id: 0,
            },
            |record| CharMapIdPair {
                platform_id: record.platform_id() as u16,
                encoding_id: record.encoding_id(),
            },
        )
    }

    pub fn current_charmap(&self) -> Option<usize> {
        self.selected_charmap_index.get()
    }

    pub fn current_charmap_encoding(&self) -> FontEncoding {
        if self.selected_encoding.get() != FontEncoding::None {
            return self.selected_encoding.get();
        }
        self.selected_charmap_index
            .get()
            .map_or(FontEncoding::Unicode, |index| {
                charmap_id_pair_to_font_encoding(self.charmap_id_pair_by_index(index))
            })
    }

    pub fn set_charmap(&self, index: Option<usize>) -> bool {
        if let Some(index) = index {
            if index >= self.charmap_count() {
                self.selected_charmap_index.set(None);
                self.selected_encoding.set(FontEncoding::None);
                return false;
            }
            self.selected_charmap_index.set(Some(index));
            self.selected_encoding.set(charmap_id_pair_to_font_encoding(
                self.charmap_id_pair_by_index(index),
            ));
        } else {
            self.selected_charmap_index.set(None);
            self.selected_encoding.set(FontEncoding::None);
        }
        true
    }

    pub fn select_charmap(&self, encoding: FontEncoding) -> bool {
        if self.raw().is_some_and(RawPostScriptFace::is_cid_type1) {
            return false;
        }
        if self.is_postscript_font() {
            if encoding == FontEncoding::Unicode {
                self.selected_encoding.set(encoding);
                self.selected_charmap_index.set(Some(0));
                return true;
            }
            if self.raw().is_some_and(RawPostScriptFace::has_encoding) {
                self.selected_encoding.set(encoding);
                self.selected_charmap_index.set(Some(1));
                return true;
            }
            return false;
        }
        for index in 0..self.charmap_count() {
            if charmap_id_pair_to_font_encoding(self.charmap_id_pair_by_index(index)) == encoding {
                self.selected_encoding.set(encoding);
                self.selected_charmap_index.set(Some(index));
                return true;
            }
        }
        false
    }

    pub(crate) fn cmap_char_to_gid(&self, index: usize, code: u32) -> u32 {
        let Some(font) = self.sfnt_opt() else {
            return 0;
        };
        if index == self.sfnt_charmap_count() {
            if let Some(map) = self.synthesized_unicode() {
                return map.map(code).map_or(0, |glyph| glyph.to_u32());
            }
        }
        if index == self.sfnt_charmap_count() + usize::from(self.synthesized_unicode().is_some()) {
            if let FaceSource::CffSfnt { raw, .. } = &self.source {
                if raw.has_glyph_names() && raw.has_encoding() {
                    return raw.encoded_char_index(code);
                }
            }
        }
        let Ok(cmap) = font.cmap() else {
            return 0;
        };
        let Some(record) = cmap.encoding_records().get(index) else {
            return 0;
        };
        let Ok(subtable) = record.subtable(cmap.offset_data()) else {
            return 0;
        };
        let gid = match &subtable {
            CmapSubtable::Format2(map) => cmap2_char_to_gid(map, code),
            _ => subtable.map_codepoint(code),
        }
        .map_or(0, |glyph| glyph.to_u32());
        if gid < self.glyph_count() {
            gid
        } else {
            0
        }
    }

    pub fn get_char_index(&self, code: u32) -> u32 {
        let unicode_or_none = matches!(
            self.selected_encoding.get(),
            FontEncoding::Unicode | FontEncoding::None
        );
        if self.is_postscript_font() {
            return if unicode_or_none {
                self.unicode_char_index(code)
            } else {
                self.raw().map_or(0, |raw| raw.encoded_char_index(code))
            };
        }
        if let Some(index) = self.selected_charmap_index.get() {
            // FT_Get_Char_Index uses the selected map, including Unicode.
            // Fontations' default Unicode mapper can prefer another record
            // (for example, a competing Windows symbol map). A missing code
            // in the selected record must also remain missing.
            return self.cmap_char_to_gid(index, code);
        }
        if unicode_or_none {
            return self.unicode_char_index(code);
        }
        0
    }

    pub fn get_char_codes_and_indices(&self, max_char: u32) -> Vec<CharCodeAndIndex> {
        let mut results: Vec<CharCodeAndIndex> = Vec::new();
        if let Some(map) = self.synthesized_unicode() {
            for (code, _) in map.iter() {
                // Fontations marks variant names (A.alt) with the high bit.
                // Enumerate each base character once, choosing its normal
                // glyph, or the variant only when no normal glyph exists.
                let code = code & 0x7fff_ffff;
                if code > max_char {
                    break;
                }
                if results.last().is_some_and(|entry| entry.char_code == code) {
                    continue;
                }
                if let Some(glyph) = map.map(code).filter(|glyph| glyph.to_u32() != 0) {
                    results.push(CharCodeAndIndex {
                        char_code: code,
                        glyph_index: glyph.to_u32(),
                    });
                }
            }
            return results;
        }
        if let FaceSource::PostScript(raw) = &self.source {
            // FreeType enumerates the selected PostScript face's charmap,
            // including its Adobe encoding when no synthesized Unicode map
            // exists. Raw Type1/CFF faces are embeddable through the public
            // font-loading API just like SFNT-wrapped outlines.
            for char_code in 0..=max_char.min(u8::MAX.into()) {
                let glyph_index = raw.encoded_char_index(char_code);
                if glyph_index != 0 {
                    results.push(CharCodeAndIndex {
                        char_code,
                        glyph_index,
                    });
                }
            }
            if results.is_empty() {
                // FT_Get_First_Char reports the .notdef slot for an empty
                // charmap; this is also the existing SFNT fallback below.
                results.push(CharCodeAndIndex {
                    char_code: 0,
                    glyph_index: 0,
                });
            }
            return results;
        }
        let glyph_count = self.glyph_count();
        let charmap = self.sfnt().charmap();
        if charmap.has_map() {
            let mut has_any = false;
            for (char_code, glyph_id) in charmap.mappings() {
                // FreeType skips mappings outside the face's actual glyph array.
                if glyph_id.to_u32() == 0 || glyph_id.to_u32() >= glyph_count {
                    continue;
                }
                has_any = true;
                if char_code > max_char {
                    break;
                }
                results.push(CharCodeAndIndex {
                    char_code,
                    glyph_index: glyph_id.to_u32(),
                });
            }
            if !has_any && results.is_empty() {
                results.push(CharCodeAndIndex {
                    char_code: 0,
                    glyph_index: 0,
                });
            }
            return results;
        }

        if let Some(map) = self.unicode_cmap2() {
            let mut has_any = false;
            for char_code in 0..=0xffff {
                if let Some(glyph) = cmap2_char_to_gid(&map, char_code)
                    .filter(|glyph| glyph.to_u32() != 0 && glyph.to_u32() < glyph_count)
                {
                    has_any = true;
                    if char_code > max_char {
                        break;
                    }
                    results.push(CharCodeAndIndex {
                        char_code,
                        glyph_index: glyph.to_u32(),
                    });
                }
            }
            if !has_any {
                results.push(CharCodeAndIndex {
                    char_code: 0,
                    glyph_index: 0,
                });
            }
            return results;
        }

        if let Ok(cmap) = self.sfnt().cmap() {
            let mut has_any = false;
            for record in cmap.encoding_records() {
                if let Ok(CmapSubtable::Format0(format0)) = record.subtable(cmap.offset_data()) {
                    for (code, &gid) in format0.glyph_id_array().iter().enumerate() {
                        if gid != 0 && u32::from(gid) < glyph_count {
                            has_any = true;
                            let char_code = code as u32;
                            if char_code <= max_char {
                                results.push(CharCodeAndIndex {
                                    char_code,
                                    glyph_index: gid as u32,
                                });
                            }
                        }
                    }
                    if !has_any && results.is_empty() {
                        results.push(CharCodeAndIndex {
                            char_code: 0,
                            glyph_index: 0,
                        });
                    }
                    return results;
                }
            }

            if let Some((_, _, subtable)) = cmap.best_subtable() {
                for (char_code, glyph_id) in subtable.iter() {
                    has_any = true;
                    if char_code > max_char {
                        continue;
                    }
                    results.push(CharCodeAndIndex {
                        char_code,
                        glyph_index: glyph_id.to_u32(),
                    });
                }
                results.sort_by_key(|result| result.char_code);
                if let Some(pos) = results
                    .iter()
                    .position(|result| result.char_code > max_char)
                {
                    results.truncate(pos);
                }
                if !has_any && results.is_empty() {
                    results.push(CharCodeAndIndex {
                        char_code: 0,
                        glyph_index: 0,
                    });
                }
            }
        }
        results
    }

    pub fn glyph_name(&self, glyph_index: u32) -> String {
        if let Some(raw) = self.raw() {
            return raw
                .glyph_name(glyph_index)
                .unwrap_or_default()
                .split('\0')
                .next()
                .unwrap_or_default()
                .to_owned();
        }
        self.sfnt()
            .glyph_names()
            .get(GlyphId::new(glyph_index))
            .map_or_else(String::new, |name| {
                name.as_str()
                    .split('\0')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
    }

    pub fn name_index(&self, name: &str) -> u32 {
        let name = name.split('\0').next().unwrap_or_default();
        if let Some(raw) = self.raw() {
            return raw.name_index(name);
        }
        if let Ok(post) = self.sfnt().post() {
            if post.num_names() != 0 {
                let count = self.glyph_count();
                return post
                    .glyph_names()
                    .take_while(|(gid, _)| gid.to_u32() < count)
                    .find(|(_, glyph_name)| *glyph_name == name)
                    .map_or(0, |(gid, _)| gid.to_u32());
            }
        }
        let names = self.sfnt().glyph_names();
        // FT_Get_Name_Index() only searches real PostScript glyph names.
        // Skrifa also provides synthesized gidN labels for nameless fonts;
        // scanning those can traverse tens of thousands of CJK glyphs for
        // every missing character while loading an ANSI font's glyph map.
        if names.source() == GlyphNameSource::Synthesized {
            return 0;
        }
        names
            .iter()
            .find(|(_, glyph_name)| glyph_name.as_str() == name)
            .map_or(0, |(glyph_id, _)| glyph_id.to_u32())
    }

    pub fn get_sfnt_table(&self, tag: u32, buffer: &mut [u8]) -> usize {
        let Some(font) = self.sfnt_opt() else {
            return 0;
        };
        let Some(data) = font.table_data(Tag::from_u32(tag)) else {
            return 0;
        };
        if buffer.is_empty() {
            return data.len();
        }
        if buffer.len() != data.len() {
            return 0;
        }
        buffer.copy_from_slice(data.as_bytes());
        buffer.len()
    }

    pub fn parse_gsub_table(&self) -> Option<CfxCttgsubTable> {
        let data = self
            .sfnt_opt()?
            .table_data(Tag::from_u32(u32::from_be_bytes(*b"GSUB")))?;
        (!data.is_empty()).then(|| CfxCttgsubTable::new(data.as_bytes()))
    }

    pub fn os2_code_page_range(&self) -> Option<[u32; 2]> {
        let os2 = self.sfnt_opt()?.os2().ok()?;
        Some([
            os2.ul_code_page_range_1().unwrap_or(0),
            os2.ul_code_page_range_2().unwrap_or(0),
        ])
    }

    pub fn os2_panose(&self) -> Option<[u8; 2]> {
        let os2 = self.sfnt_opt()?.os2().ok()?;
        let panose = os2.panose_10();
        Some([panose[0], panose[1]])
    }

    pub fn font_style(&self) -> u32 {
        let mut style = 0;
        if self.is_bold() {
            style |= FONT_STYLE_FORCE_BOLD;
        }
        if self.is_italic() {
            style |= FONT_STYLE_ITALIC;
        }
        if self.is_fixed_width() {
            style |= FONT_STYLE_FIXED_PITCH;
        }
        if self
            .os2_code_page_range()
            .is_some_and(|range| range[0] & (1 << 31) != 0)
        {
            style |= FONT_STYLE_SYMBOLIC;
        }
        if self.os2_panose().is_some_and(|panose| {
            panose[0] == 2 && ((panose[1] > 1 && panose[1] < 10) || panose[1] > 13)
        }) {
            style |= FONT_STYLE_SERIF;
        }
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcd_filter_matches_scatter_rounding_and_edges() {
        let mut random = 1u32;
        for width in (0..=130).chain([255, 256, 257, 2048]) {
            for pattern in 0..8 {
                let input: Vec<u8> = (0..width)
                    .map(|x| {
                        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                        match pattern {
                            0 => 0,
                            1 => 255,
                            2 => x as u8,
                            3 => {
                                if x % 2 == 0 {
                                    255
                                } else {
                                    0
                                }
                            }
                            4 => {
                                if x == 0 {
                                    255
                                } else {
                                    0
                                }
                            }
                            5 => {
                                if x + 1 == width {
                                    255
                                } else {
                                    0
                                }
                            }
                            _ => (random >> 24) as u8,
                        }
                    })
                    .collect();
                let mut expected = vec![0u8; width];
                for (x, &coverage) in input.iter().enumerate() {
                    for (tap, weight) in [8u32, 77, 86, 77, 8].into_iter().enumerate() {
                        let target = x as isize + tap as isize - 2;
                        if target >= 0 && (target as usize) < width {
                            expected[target as usize] = expected[target as usize]
                                .saturating_add(((u32::from(coverage) * weight + 85) >> 8) as u8);
                        }
                    }
                }
                let mut actual = vec![193; width + 4];
                filter_lcd_row(&input, &mut actual[2..width + 2]);
                assert_eq!(
                    &actual[2..width + 2],
                    expected,
                    "width={width}, pattern={pattern}"
                );
                assert_eq!(&actual[..2], &[193; 2]);
                assert_eq!(&actual[width + 2..], &[193; 2]);
            }
        }
    }

    #[test]
    fn retained_sfnt_outlines_survive_moves_and_charmap_changes() {
        for data in [
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf")
                .as_slice(),
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/roboto.ttf")
                .as_slice(),
        ] {
            let bytes = Rc::new(data.to_vec());
            let face = CfxOwnedFace::new(Rc::clone(&bytes), 0).unwrap();
            let expected: Vec<_> = ['A', 'g', ' ']
                .into_iter()
                .map(|ch| {
                    let glyph = face.face().unicode_char_index(ch as u32);
                    (
                        glyph,
                        face.face().glyph_bbox(glyph),
                        face.face().char_bbox(glyph as i32),
                    )
                })
                .collect();
            drop(bytes);
            // Retained outline tables borrow the cart, not the face's address.
            let mut moved = Some(face);
            let mut face = moved.take().unwrap();
            assert!(face.with_face_mut(|face| face.select_charmap(FontEncoding::Unicode)));
            for (glyph, glyph_bounds, char_bounds) in expected {
                assert_eq!(face.face().glyph_bbox(glyph), glyph_bounds);
                assert_eq!(face.face().char_bbox(glyph as i32), char_bounds);
            }
            assert_eq!(face.face().font_glyph_bbox(face.face().glyph_count()), None);
        }
    }

    #[test]
    fn sfnt_without_outline_tables_is_rejected() {
        // A readable SFNT directory is not sufficient for the rendering face:
        // native FreeType rejects this font, allowing PDF font substitution.
        let data = [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(FontRef::new(&data).is_ok());
        assert_eq!(
            CfxFace::new(&data, 0).err(),
            Some(CfxFaceError::InvalidFont)
        );
    }

    #[test]
    fn truncated_required_sfnt_tables_are_rejected_but_optional_tables_are_not() {
        let bytes = include_bytes!("../../tests/fixtures/hint-optional.ttf");
        let font = FontRef::new(bytes).unwrap();
        let records = font.table_directory().table_records();
        for tag in [b"head", b"hmtx", b"loca", b"glyf"] {
            let record = records
                .iter()
                .find(|r| r.tag.get() == Tag::new(tag))
                .unwrap();
            let end = record.offset() as usize + record.length() as usize / 2;
            let truncated = &bytes[..end];
            assert!(FontRef::new(truncated).is_ok(), "{tag:?}");
            assert_eq!(
                CfxFace::new(truncated, 0).err(),
                Some(CfxFaceError::InvalidFont),
                "{tag:?}"
            );
        }
        // Losing optional name/post tables must not discard valid outlines.
        let name = records
            .iter()
            .find(|r| r.tag.get() == Tag::new(b"name"))
            .unwrap();
        let complete = CfxFace::new(bytes, 0).unwrap();
        let truncated = CfxFace::new(&bytes[..name.offset() as usize], 0).unwrap();
        let glyph = complete.unicode_char_index(u32::from(b'A'));
        assert_ne!(glyph, 0);
        assert_eq!(truncated.unicode_char_index(u32::from(b'A')), glyph);
        assert_eq!(
            complete
                .load_glyph_path(glyph, 0, false, None)
                .unwrap()
                .points(),
            truncated
                .load_glyph_path(glyph, 0, false, None)
                .unwrap()
                .points()
        );
        let draw = |face: &CfxFace<'_>| {
            face.render_glyph(
                glyph,
                false,
                false,
                CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0),
                0,
                FontAntiAliasingMode::Normal,
                None,
            )
            .unwrap()
        };
        let expected = draw(&complete);
        let actual = draw(&truncated);
        assert_eq!(expected.top_left_origin(), actual.top_left_origin());
        assert_eq!(expected.bitmap().buffer(), actual.bitmap().buffer());
    }

    #[test]
    fn headless_otto_uses_cff_metrics_outlines_and_optional_cmap() {
        fn wrap(tables: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
            let mut bytes = vec![0; 12 + 16 * tables.len()];
            bytes[..4].copy_from_slice(b"OTTO");
            bytes[4..6].copy_from_slice(&(tables.len() as u16).to_be_bytes());
            for (index, (tag, data)) in tables.iter().enumerate() {
                bytes.resize((bytes.len() + 3) & !3, 0);
                let offset = bytes.len() as u32;
                let record = 12 + 16 * index;
                bytes[record..record + 4].copy_from_slice(*tag);
                bytes[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
                bytes[record + 12..record + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
                bytes.extend_from_slice(data);
            }
            bytes
        }
        let cff =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/foxit_fixed.cff");
        let raw = CfxFace::new(cff, 0).unwrap();
        let a = raw.name_index("A");
        let b = raw.name_index("B");
        let bytes = wrap(&[(b"CFF ", cff)]);
        let face = CfxFace::new(&bytes, 0).unwrap();
        assert!(face.is_tt_ot());
        assert_eq!(face.font_format(), "CFF");
        assert_eq!(face.units_per_em(), raw.units_per_em());
        assert_eq!(face.glyph_count(), raw.glyph_count());
        assert_eq!(face.family_name(), raw.family_name());
        assert_eq!(face.glyph_tt_width(a), raw.glyph_tt_width(a));
        assert_eq!(face.font_glyph_bbox(a), raw.font_glyph_bbox(a));
        assert_eq!(face.get_char_index(65), a);
        assert!(face.select_charmap(FontEncoding::Unicode));
        assert_eq!(face.get_char_index(65), a);
        assert!(face.select_charmap(FontEncoding::AdobeCustom));
        assert_eq!(face.get_char_index(65), a);
        assert_eq!(face.charmap_count(), 2);
        assert_eq!(
            face.get_sfnt_table(u32::from_be_bytes(*b"CFF "), &mut []),
            cff.len()
        );
        let render = |face: &CfxFace<'_>, glyph| {
            face.render_glyph(
                glyph,
                false,
                false,
                CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0),
                0,
                FontAntiAliasingMode::Normal,
                None,
            )
            .unwrap()
        };
        assert_eq!(
            render(&face, a).bitmap().buffer(),
            render(&raw, a).bitmap().buffer()
        );
        // An existing Unicode map takes priority over CFF glyph names. Map A
        // to the B outline deliberately so a discarded cmap is detectable.
        let mut cmap = vec![0, 0, 0, 1, 0, 3, 0, 1, 0, 0, 0, 12];
        cmap.extend(
            [
                4u16,
                32,
                0,
                4,
                4,
                1,
                0,
                65,
                65535,
                0,
                65,
                65535,
                (b as u16).wrapping_sub(65),
                1,
                0,
                0,
            ]
            .into_iter()
            .flat_map(u16::to_be_bytes),
        );
        let bytes = wrap(&[(b"CFF ", cff), (b"cmap", &cmap)]);
        let mapped = CfxFace::new(&bytes, 0).unwrap();
        assert_eq!(mapped.charmap_count(), 2); // actual Unicode + CFF encoding
        assert_eq!(mapped.unicode_char_index(65), b);
        assert!(mapped.select_charmap(FontEncoding::Unicode));
        assert_eq!(mapped.get_char_index(65), b);
        assert_eq!(mapped.name_index("A"), a);
        assert_eq!(
            mapped.get_char_codes_and_indices(65),
            [CharCodeAndIndex {
                char_code: 65,
                glyph_index: b,
            }]
        );
        assert!(mapped.select_charmap(FontEncoding::AdobeCustom));
        assert_eq!(mapped.get_char_index(65), a);
        let cid_cff =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/minion.cff");
        let raw_cid = CfxFace::new(cid_cff, 0).unwrap();
        let bytes = wrap(&[(b"CFF ", cid_cff)]);
        let cid = CfxFace::new(&bytes, 0).unwrap();
        assert!(!cid.has_glyph_names());
        assert_eq!(cid.charmap_count(), 0);
        assert_eq!(cid.glyph_tt_width(54), raw_cid.glyph_tt_width(54));
        assert_eq!(
            render(&cid, 54).bitmap().buffer(),
            render(&raw_cid, 54).bitmap().buffer()
        );
        // A present but invalid head still fails; the pure-CFF exception must
        // not mask invalid unitsPerEm or a damaged CFF table.
        for bytes in [
            wrap(&[(b"CFF ", cff), (b"head", &[0; 54])]),
            wrap(&[(b"CFF ", &[1, 0, 4, 4])]),
            wrap(&[(b"cmap", &cmap)]),
        ] {
            assert_eq!(
                CfxFace::new(&bytes, 0).err(),
                Some(CfxFaceError::InvalidFont)
            );
        }
    }

    #[test]
    fn postscript_large_em_preserves_paths_metrics_and_masks() {
        let type1 = include_str!("../../tests/fixtures/triangle.pfa");
        let large_type1 = type1.replace("[0.001 0 0 0.001 0 0]", "[0.00001 0 0 0.00001 0 0]");
        // Original CFF1: .notdef and a triangular A, with a 0.00001 FontMatrix.
        let large_cff = [
            1, 0, 4, 1, 0, 1, 1, 1, 16, 80, 100, 102, 111, 120, 105, 100, 84, 114, 105, 97, 110,
            103, 108, 101, 0, 1, 1, 1, 27, 30, 28, 95, 139, 139, 30, 28, 95, 139, 139, 12, 7, 139,
            139, 248, 136, 249, 80, 5, 198, 15, 139, 223, 18, 201, 17, 0, 0, 0, 0, 0, 0, 34, 0, 2,
            1, 1, 4, 17, 248, 236, 14, 248, 236, 139, 22, 248, 136, 6, 252, 136, 249, 80, 5, 14,
        ];
        let mut ordinary_cff = large_cff;
        // BCD real operands 1E-5 -> 1E-3; the rest of the font is identical.
        ordinary_cff[31] = 0x3f;
        ordinary_cff[36] = 0x3f;
        // Type 1's fixed-point matrix normalization rounds the scale to 100055.
        for (ordinary, large, units, width) in [
            (type1.as_bytes(), large_type1.as_bytes(), 100_055, 5),
            (ordinary_cff.as_slice(), large_cff.as_slice(), 100_000, 6),
        ] {
            let ordinary = CfxFace::new(ordinary, 0).unwrap();
            let large = CfxFace::new(large, 0).unwrap();
            assert_eq!(ordinary.units_per_em(), 1000);
            assert_eq!(large.units_per_em(), units);
            let size_ratio = units as f32 / 1000.0;
            let glyph = large.name_index("A");
            assert_ne!(glyph, 0);
            assert_eq!(large.glyph_width(glyph, 0, None), width);
            assert_eq!(large.font_glyph_bbox(glyph), Some(FxRect::new(0, 0, 5, 7)));
            let expected = ordinary.load_glyph_path(glyph, 0, false, None).unwrap();
            let actual = large.load_glyph_path(glyph, 0, false, None).unwrap();
            assert_eq!(actual.points().len(), expected.points().len());
            for (actual, expected) in actual.points().iter().zip(expected.points()) {
                assert_eq!(actual.point_type, expected.point_type);
                assert_eq!(actual.close_figure, expected.close_figure);
                assert!((actual.point.x * size_ratio - expected.point.x).abs() < 0.00001);
                assert!((actual.point.y * size_ratio - expected.point.y).abs() < 0.00001);
            }
            let render = |face: &CfxFace<'_>, size| {
                face.render_glyph(
                    glyph,
                    false,
                    false,
                    CfxMatrix::new(size, 0.0, 0.0, size, 0.0, 0.0),
                    0,
                    FontAntiAliasingMode::Normal,
                    None,
                )
                .unwrap()
            };
            // One eighth of a pixel per font unit avoids a rounding boundary
            // at the triangle's right/top edge when comparing mask extents.
            let expected = render(&ordinary, 125.0);
            let actual = render(&large, units as f32 / 8.0);
            assert_eq!(actual.top_left_origin(), expected.top_left_origin());
            assert_eq!(actual.bitmap().width(), expected.bitmap().width());
            assert_eq!(actual.bitmap().height(), expected.bitmap().height());
            assert!(actual.bitmap().buffer().contains(&255));
            assert_eq!(actual.bitmap().buffer(), expected.bitmap().buffer());
        }
    }

    #[test]
    fn name_lookup_requires_real_glyph_names() {
        let original = include_bytes!("../../tests/fixtures/hint-optional.ttf");
        let source = FontRef::new(original).unwrap();
        let records = source.table_directory().table_records();
        let post_record = 12
            + 16 * records
                .iter()
                .position(|record| record.tag.get() == Tag::new(b"post"))
                .unwrap();
        let post_offset = records
            .iter()
            .find(|record| record.tag.get() == Tag::new(b"post"))
            .unwrap()
            .offset() as usize;
        // Both post version 3 and an absent post table mean there are no
        // PostScript names. Skrifa's diagnostic gidN labels are not names
        // accepted by FreeType's FT_Get_Name_Index().
        for missing_post in [false, true] {
            let mut bytes = original.to_vec();
            bytes[post_offset..post_offset + 4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
            if missing_post {
                bytes[post_record..post_record + 4].copy_from_slice(b"POST");
            }
            let face = CfxFace::new(&bytes, 0).unwrap();
            assert!(!face.has_glyph_names());
            assert_eq!(face.name_index("gid1"), 0);
            assert_eq!(face.name_index("A"), 0);
            assert!(face.load_glyph_path(1, 0, false, None).is_some());
        }
        // Version 1 uses the standard Macintosh names, without a custom list.
        let mut bytes = original.to_vec();
        bytes[post_offset..post_offset + 4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        let face = CfxFace::new(&bytes, 0).unwrap();
        assert!(face.has_glyph_names());
        assert_eq!(face.name_index(".null"), 1);
    }

    #[test]
    fn raw_postscript_faces_draw_paths_and_agg_masks() {
        assert_eq!(CfxFace::unicode_from_adobe_name("A\0tail"), u32::from(b'A'));
        let cff =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/foxit_fixed.cff")
                .as_slice();
        let type1 = include_bytes!("../../tests/fixtures/triangle.pfa").as_slice();
        for (data, format) in [(cff, "CFF"), (type1, "Type 1")] {
            let face = CfxFace::new(data, 0).unwrap();
            assert!(!face.is_tt_ot());
            assert_eq!(face.font_format(), format);
            assert_eq!(face.charmap_count(), 2);
            let glyph = face.get_char_index(u32::from(b'A'));
            assert!(glyph > 0);
            assert_eq!(face.glyph_name(glyph), "A");
            assert_eq!(face.name_index("A"), glyph);
            assert_eq!(face.name_index("A\0tail"), glyph);
            assert!(face.select_charmap(FontEncoding::AdobeCustom));
            assert_eq!(face.get_char_index(u32::from(b'A')), glyph);
            assert_eq!(face.get_char_index(256), 0);
            assert!(face.select_charmap(FontEncoding::Unicode));
            assert_eq!(face.get_char_index(u32::from(b'A')), glyph);
            // Public font embedding enumerates this map to construct PDF
            // widths and ToUnicode, for raw CFF and raw Type 1 alike.
            let mappings = face.get_char_codes_and_indices(0xffff);
            assert!(mappings
                .iter()
                .any(|entry| entry.char_code == u32::from(b'A') && entry.glyph_index == glyph));
            assert!(mappings
                .windows(2)
                .all(|pair| pair[0].char_code < pair[1].char_code));
            assert!(face
                .get_char_codes_and_indices(u32::from(b'A'))
                .iter()
                .all(|entry| entry.char_code <= u32::from(b'A')));
            assert!(face.glyph_tt_width(glyph) > 0);
            assert!(face.load_glyph_path(glyph, 0, false, None).is_some());
            let bitmap = face
                .render_glyph(
                    glyph,
                    false,
                    false,
                    CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0),
                    0,
                    FontAntiAliasingMode::Normal,
                    None,
                )
                .unwrap();
            assert!(bitmap
                .bitmap()
                .buffer()
                .iter()
                .any(|&coverage| coverage != 0));
            assert!(face.os2_code_page_range().is_none());
            assert_eq!(
                face.get_sfnt_table(u32::from_be_bytes(*b"head"), &mut []),
                0
            );
            assert!(face.load_glyph_path(u32::MAX, 0, false, None).is_none());
        }
        let face = CfxFace::new(type1, 0).unwrap();
        let glyph = face.name_index("A");
        assert_eq!(face.glyph_tt_width(glyph), 600);
        assert_eq!(
            face.font_glyph_bbox(glyph),
            Some(FxRect::new(0, 0, 500, 700))
        );
    }

    #[test]
    fn format2_maps_single_and_double_byte_codes_without_an_alternate_map() {
        let original = include_bytes!("../../tests/fixtures/hint-optional.ttf");
        let source = FontRef::new(original).unwrap();
        let records = source.table_directory().table_records();
        let cmap_record = 12
            + 16 * records
                .iter()
                .position(|record| record.tag.get() == Tag::new(b"cmap"))
                .unwrap();
        let post_offset = records
            .iter()
            .find(|record| record.tag.get() == Tag::new(b"post"))
            .unwrap()
            .offset() as usize;
        // Four subheaders, followed by one-byte and shared two-byte glyph arrays.
        let mut table = vec![0; 518 + 32 + 512 + 8];
        let mut word = |offset: usize, value: u16| {
            table[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        };
        word(0, 2);
        word(2, 1070);
        for (lead, key) in [(0x81, 8), (0x82, 8), (0x83, 16), (0x84, 24)] {
            word(6 + lead * 2, key);
        }
        for (index, values) in [
            [0, 256, 0xffff, 26], // glyph minus one, offset relative to this word
            [0x30, 4, 2, 530],    // wrap 65535 + 2 to glyph 1
            [0x40, 1, 1, 0],      // zero offset means no mapping
            [0x50, 1, 0, 65535],  // out-of-bounds glyph array
        ]
        .into_iter()
        .enumerate()
        {
            for (field, value) in values.into_iter().enumerate() {
                word(518 + index * 8 + field * 2, value);
            }
        }
        for (code, gid) in [(65, 2), (66, 0), (67, 3), (68, 65535), (0x81, 2)] {
            word(550 + code * 2, gid);
        }
        for (index, gid) in [65535, 0, 65534, 1].into_iter().enumerate() {
            word(1062 + index * 2, gid);
        }
        let make_font = |table: &[u8]| {
            let mut bytes = original.to_vec();
            bytes[post_offset..post_offset + 4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
            let mut cmap = vec![0, 0, 0, 1, 0, 3, 0, 1, 0, 0, 0, 12];
            cmap.extend_from_slice(table);
            let offset = bytes.len() as u32;
            bytes[cmap_record + 8..cmap_record + 12].copy_from_slice(&offset.to_be_bytes());
            bytes[cmap_record + 12..cmap_record + 16]
                .copy_from_slice(&(cmap.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&cmap);
            bytes
        };
        let bytes = make_font(&table);
        let face = CfxFace::new(&bytes, 0).unwrap();
        assert_eq!(face.charmap_count(), 1);
        assert!(!face.has_glyph_names());
        assert!(face.select_charmap(FontEncoding::Unicode));
        for code in [65, 0x8130, 0x8230] {
            assert_eq!(face.cmap_char_to_gid(0, code), 1, "{code:x}");
            assert_eq!(face.unicode_char_index(code), 1, "{code:x}");
            assert_eq!(face.get_char_index(code), 1, "{code:x}");
        }
        for code in [
            0,
            64,
            66,
            67,
            68,
            0x81,
            0x82,
            0x141,
            0x812f,
            0x8131,
            0x8132,
            0x8133,
            0x8134,
            0x8340,
            0x8450,
            0xffff,
            0x10000,
            u32::MAX,
        ] {
            assert_eq!(face.get_char_index(code), 0, "{code:x}");
        }
        let mappings = face.get_char_codes_and_indices(u32::MAX);
        assert_eq!(
            mappings,
            [65, 0x8130, 0x8230].map(|char_code| CharCodeAndIndex {
                char_code,
                glyph_index: 1,
            })
        );
        assert_eq!(face.get_char_codes_and_indices(0x81ff), mappings[..2]);
        assert!(face.get_char_codes_and_indices(64).is_empty());
        // Do not read beyond the declared table length, even when font bytes
        // after that boundary happen to contain a usable glyph ID.
        for length in [0u16, 518, 549, 681, 1063, u16::MAX] {
            let mut truncated = table.clone();
            truncated[2..4].copy_from_slice(&length.to_be_bytes());
            let bytes = make_font(&truncated);
            let face = CfxFace::new(&bytes, 0).unwrap();
            assert!(face.select_charmap(FontEncoding::Unicode));
            assert_eq!(face.get_char_index(0x8130), 0, "length={length}");
        }
        assert!(face.load_glyph_path(1, 0, false, None).is_some());
    }

    #[test]
    fn sfnt_selected_unicode_charmap_does_not_use_competing_symbol_or_mac_map() {
        let original =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let source = FontRef::new(original).unwrap();
        let records = source.table_directory().table_records();
        let cmap_record = 12
            + 16 * records
                .iter()
                .position(|record| record.tag.get() == Tag::new(b"cmap"))
                .unwrap();
        let post_offset = records
            .iter()
            .find(|record| record.tag.get() == Tag::new(b"post"))
            .unwrap()
            .offset() as usize;
        let original_face = CfxFace::new(original, 0).unwrap();
        let glyphs = ['A', 'B', 'C'].map(|ch| original_face.get_char_index(ch as u32) as u16);
        assert!(glyphs.iter().all(|&glyph| glyph != 0));
        // A sorted cmap has Macintosh, Windows symbol, then Windows Unicode.
        // Also vary record order so map selection cannot accidentally rely on
        // the first record or the library's default preferred map.
        for order in [[0usize, 1, 2], [2, 0, 1], [1, 2, 0]] {
            let definitions = [
                (1u16, 0u16, glyphs[2], glyphs[2]),
                (3, 0, glyphs[1], glyphs[1]),
                (3, 1, glyphs[0], 0),
            ];
            let mut cmap = vec![0, 0, 0, 3];
            for (index, &entry) in order.iter().enumerate() {
                let (platform, encoding, _, _) = definitions[entry];
                cmap.extend_from_slice(&platform.to_be_bytes());
                cmap.extend_from_slice(&encoding.to_be_bytes());
                cmap.extend_from_slice(&(28u32 + index as u32 * 14).to_be_bytes());
            }
            for &entry in &order {
                let (_, _, first, second) = definitions[entry];
                // Valid format 6: A and B are the only character slots.
                for value in [6u16, 14, 0, 65, 2, first, second] {
                    cmap.extend_from_slice(&value.to_be_bytes());
                }
            }
            let mut bytes = original.to_vec();
            bytes[post_offset..post_offset + 4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
            let offset = bytes.len() as u32;
            bytes[cmap_record + 8..cmap_record + 12].copy_from_slice(&offset.to_be_bytes());
            bytes[cmap_record + 12..cmap_record + 16]
                .copy_from_slice(&(cmap.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&cmap);
            let face = CfxFace::new(&bytes, 0).unwrap();
            assert_eq!(face.charmap_count(), 3);
            for (index, &entry) in order.iter().enumerate() {
                let (_, _, first, second) = definitions[entry];
                assert!(face.set_charmap(Some(index)));
                assert_eq!(face.get_char_index('A' as u32), u32::from(first));
                assert_eq!(face.get_char_index('B' as u32), u32::from(second));
                assert_eq!(face.get_char_index('C' as u32), 0);
            }
            assert!(face.select_charmap(FontEncoding::Unicode));
            assert_eq!(face.get_char_index('A' as u32), u32::from(glyphs[0]));
            assert_eq!(face.get_char_index('B' as u32), 0);
        }
    }

    #[test]
    fn sfnt_unicode_from_post_names_preserves_glyph_ids_and_existing_maps() {
        let original =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let source = FontRef::new(original).unwrap();
        let records = source.table_directory().table_records();
        let cmap_record = 12
            + 16 * records
                .iter()
                .position(|r| r.tag.get() == Tag::new(b"cmap"))
                .unwrap();
        let post_record = 12
            + 16 * records
                .iter()
                .position(|r| r.tag.get() == Tag::new(b"post"))
                .unwrap();
        let original_face = CfxFace::new(original, 0).unwrap();
        let original_a = original_face.get_char_index('A' as u32);
        let mut without_cmap = original.to_vec();
        without_cmap[cmap_record..cmap_record + 4].copy_from_slice(b"CMAP");
        let face = CfxFace::new(&without_cmap, 0).unwrap();
        assert_eq!(face.charmap_count(), 1);
        assert_eq!(
            face.charmap_id_pair_by_index(0),
            CharMapIdPair {
                platform_id: PLATFORM_WINDOWS,
                encoding_id: WINDOWS_ENCODING_UNICODE,
            }
        );
        assert!(face.select_charmap(FontEncoding::Unicode));
        assert_eq!(face.current_charmap(), Some(0));
        assert_eq!(face.get_char_index('A' as u32), original_a);
        assert_eq!(face.cmap_char_to_gid(0, 'A' as u32), original_a);
        assert_eq!(face.get_char_index(0x10ffff), 0);
        for size in [13.5, 50.0] {
            let render = |f: &CfxFace<'_>| {
                f.render_glyph(
                    original_a,
                    false,
                    false,
                    CfxMatrix::new(size, 0.0, 0.0, size, 0.0, 0.0),
                    0,
                    FontAntiAliasingMode::Normal,
                    None,
                )
                .unwrap()
            };
            let expected = render(&original_face);
            let actual = render(&face);
            assert_eq!(expected.top_left_origin(), actual.top_left_origin());
            assert_eq!(expected.bitmap().buffer(), actual.bitmap().buffer());
        }

        // A valid Mac-only cmap stays selectable. The synthesized Unicode map
        // is appended, rather than changing the meaning of the existing map.
        let mut mac_only = without_cmap.clone();
        mac_only[cmap_record..cmap_record + 4].copy_from_slice(b"cmap");
        let mut cmap = vec![0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 12, 0, 0, 1, 6, 0, 0];
        cmap.resize(12 + 262, 0);
        cmap[18 + b'A' as usize] = 1;
        let offset = mac_only.len() as u32;
        mac_only[cmap_record + 8..cmap_record + 12].copy_from_slice(&offset.to_be_bytes());
        mac_only[cmap_record + 12..cmap_record + 16]
            .copy_from_slice(&(cmap.len() as u32).to_be_bytes());
        mac_only.extend_from_slice(&cmap);
        let mac_face = CfxFace::new(&mac_only, 0).unwrap();
        assert_eq!(mac_face.charmap_count(), 2);
        assert!(mac_face.select_charmap(FontEncoding::AppleRoman));
        assert_eq!(mac_face.current_charmap(), Some(0));
        assert_eq!(mac_face.get_char_index('A' as u32), 1);
        assert!(mac_face.select_charmap(FontEncoding::Unicode));
        assert_eq!(mac_face.current_charmap(), Some(1));
        assert_eq!(mac_face.get_char_index('A' as u32), original_a);
        // Existing Unicode and symbol maps suppress synthesis completely.
        assert!(original_face.synthesized_unicode().is_none());
        let mut symbol = mac_only.clone();
        symbol[offset as usize + 4..offset as usize + 8].copy_from_slice(&[0, 3, 0, 0]);
        let symbol_face = CfxFace::new(&symbol, 0).unwrap();
        assert_eq!(symbol_face.charmap_count(), 1);
        assert!(symbol_face.synthesized_unicode().is_none());
        assert!(!symbol_face.select_charmap(FontEncoding::Unicode));
        assert!(symbol_face.select_charmap(FontEncoding::Symbol));
        assert_eq!(symbol_face.get_char_index('A' as u32), 1);

        // Out-of-order names, an unreadable Pascal string and a variant name
        // must not shift later glyph IDs or produce duplicate enumerated chars.
        let names: [&[u8]; 8] = [
            b".notdef",
            b"A.alt",
            b"A",
            b"u1F600",
            b"Omega",
            b"space",
            &[255],
            b"B",
        ];
        let mut bytes = without_cmap;
        let mut post = vec![0u8; 32];
        post[..4].copy_from_slice(&0x0002_0000u32.to_be_bytes());
        let count = original_face.glyph_count() as u16;
        post.extend_from_slice(&count.to_be_bytes());
        for gid in 0..count {
            post.extend_from_slice(&(if gid < 8 { 258 + gid } else { 0 }).to_be_bytes());
        }
        for name in names {
            post.push(name.len() as u8);
            post.extend_from_slice(name);
        }
        let post_offset = bytes.len() as u32;
        bytes[post_record + 8..post_record + 12].copy_from_slice(&post_offset.to_be_bytes());
        bytes[post_record + 12..post_record + 16]
            .copy_from_slice(&(post.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&post);
        let named = CfxFace::new(&bytes, 0).unwrap();
        assert_eq!(named.get_char_index('A' as u32), 2); // base before variant
        assert_eq!(named.get_char_index('B' as u32), 7); // not 6 after bad name
        assert_eq!(named.get_char_index(0x1f600), 3);
        assert_eq!(named.get_char_index(0x03a9), 4); // Omega AGL alias
        assert_eq!(named.name_index("B"), 7);
        let codes = named.get_char_codes_and_indices(u32::MAX);
        assert!(codes.windows(2).all(|w| w[0].char_code < w[1].char_code));
        assert_eq!(
            codes.iter().filter(|c| c.char_code == 'A' as u32).count(),
            1
        );
        assert_eq!(
            codes
                .iter()
                .find(|c| c.char_code == 'A' as u32)
                .unwrap()
                .glyph_index,
            2
        );
        assert_eq!(
            named.get_char_codes_and_indices(0x41),
            codes
                .into_iter()
                .filter(|c| c.char_code <= 0x41)
                .collect::<Vec<_>>()
        );

        // No recognizable names means no synthesized map, preserving native's
        // sequential-GID fallback for these damaged or custom-encoded fonts.
        bytes[post_offset as usize..post_offset as usize + 4]
            .copy_from_slice(&0x0003_0000u32.to_be_bytes());
        let unnamed = CfxFace::new(&bytes, 0).unwrap();
        assert_eq!(unnamed.charmap_count(), 0);
        assert!(unnamed.synthesized_unicode().is_none());
    }

    #[test]
    fn sfnt_face_rejects_invalid_units_per_em_before_glyph_loading() {
        let original = include_bytes!("../../tests/fixtures/hint-optional.ttf");
        let font = FontRef::new(original).unwrap();
        let head = font
            .table_directory()
            .table_records()
            .iter()
            .find(|record| record.tag.get() == Tag::new(b"head"))
            .unwrap()
            .offset() as usize;
        for units in [0u16, 1, 15, 16385, u16::MAX] {
            let mut data = original.to_vec();
            data[head + 18..head + 20].copy_from_slice(&units.to_be_bytes());
            // A parsed SFNT and an outline collection do not imply a usable em.
            assert!(FontRef::new(&data)
                .unwrap()
                .outline_glyphs()
                .format()
                .is_some());
            assert_eq!(
                CfxFace::new(&data, 0).err(),
                Some(CfxFaceError::InvalidFont)
            );
        }
        for units in [16u16, 1000, 16384] {
            let mut data = original.to_vec();
            data[head + 18..head + 20].copy_from_slice(&units.to_be_bytes());
            let face = CfxFace::new(&data, 0).unwrap();
            assert_eq!(face.units_per_em(), u32::from(units));
            let glyph = face.get_char_index(u32::from(b'A'));
            assert!(face.load_glyph_path(glyph, 0, false, None).is_some());
        }
    }

    #[test]
    fn cid_cff_maps_public_cid_to_outline_glyph() {
        let data = include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/minion.cff");
        let face = CfxFace::new(data, 0).unwrap();
        assert!(!face.has_glyph_names());
        // This subset stores the outline for CID 54 at raw glyph index 3.
        assert!(face.glyph_count() > 54);
        assert!(face.load_glyph_path(54, 0, false, None).is_some());
        assert!(face.font_glyph_bbox(54).is_some());
        assert!(face.glyph_tt_width(54) > 0);
        assert!(face.load_glyph_path(2, 0, false, None).is_none());
    }

    #[test]
    fn ordinary_sfnt_outlines_stay_unhinted_when_interpreter_is_only_preferred() {
        for data in [
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/times_bold.ttf")
                .as_slice(),
            include_bytes!("../../tests/fixtures/hint-optional.ttf").as_slice(),
        ] {
            let face = CfxFace::new(data, 0).unwrap();
            assert!(face.outlines().prefer_interpreter());
            assert!(!face.is_tricky());
            let glyph = face.get_char_index(u32::from(b'A'));
            assert!(face.load_glyph_path(glyph, 0, false, None).is_some());
            assert!(face
                .render_glyph(
                    glyph,
                    false,
                    false,
                    CfxMatrix::new(13.5, 0.0, 0.0, 13.5, 0.0, 0.0),
                    0,
                    FontAntiAliasingMode::Normal,
                    None,
                )
                .is_some());
            assert!(face.hinter.borrow().is_none());
        }
    }

    #[test]
    fn synthetic_bold_paths_expand_at_native_load_strength_after_italic_skew() {
        let face =
            CfxFace::new(include_bytes!("../../tests/fixtures/hint-optional.ttf"), 0).unwrap();
        let glyph = face.get_char_index(u32::from(b'A'));
        let bounds = |path: &CfxPath| {
            path.points().iter().fold(
                [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
                |r, p| {
                    [
                        r[0].min(p.point.x),
                        r[1].min(p.point.y),
                        r[2].max(p.point.x),
                        r[3].max(p.point.y),
                    ]
                },
            )
        };
        let plain = face.load_glyph_path(glyph, 0, false, None).unwrap();
        assert_eq!(bounds(&plain), [0.0, 0.0, 0.4, 0.7]);
        let mut subst = CfxSubstFont::new();
        subst.set_weight(400);
        assert_eq!(
            bounds(&face.load_glyph_path(glyph, 0, false, Some(&subst)).unwrap()),
            bounds(&plain)
        );
        subst.set_weight(700);
        let bold = face.load_glyph_path(glyph, 0, false, Some(&subst)).unwrap();
        // Native LoadGlyphPath sets 64 ppem, emboldens its 26.6 outline by 70
        // units for weight 700, then divides by 64*64 to return font-em paths.
        let strength = 70.0 / 4096.0;
        let bold_bounds = bounds(&bold);
        for (actual, expected) in
            bold_bounds
                .into_iter()
                .zip([0.0, 0.0, 0.4 + strength, 0.7 + strength])
        {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        }
        subst.set_italic_angle(-12);
        let italic_bold = face.load_glyph_path(glyph, 0, false, Some(&subst)).unwrap();
        let italic_bounds = bounds(&italic_bold);
        // Applying the italic transform after bold expansion would leave the
        // lower-left corner at zero. Native expands the already slanted edges.
        assert!(italic_bounds[0] < -0.001);
        assert!((italic_bounds[1] - 0.0).abs() < 1e-6);
        assert!((italic_bounds[3] - (0.7 + strength)).abs() < 1e-6);
        assert!(italic_bounds[2] > bold_bounds[2]);
    }

    #[test]
    fn normalized_outline_edges_preserve_exact_and_fractional_device_bounds() {
        for (bytes, width_units) in [
            (
                include_bytes!("../../tests/fixtures/hint-optional.ttf").as_slice(),
                400.0,
            ),
            (
                include_bytes!("../../tests/fixtures/hint-reliant.ttf").as_slice(),
                600.0,
            ),
        ] {
            let face = CfxFace::new(bytes, 0).unwrap();
            let glyph = face.get_char_index(u32::from(b'A'));
            let path = face.load_glyph_path(glyph, 0, false, None).unwrap();
            let right = path
                .points()
                .iter()
                .map(|point| point.point.x)
                .fold(0.0, f32::max);
            let top = path
                .points()
                .iter()
                .map(|point| point.point.y)
                .fold(0.0, f32::max);
            assert_eq!(right, width_units / 1000.0);
            assert_eq!(top, 700.0 / 1000.0);
            // Fractional outlines retain the independently specified geometry
            // within f32 arithmetic precision. An outward allocation box may
            // include a rounding fringe; it is not a painted-pixel assertion.
            for size in [10.0f32, 13.5, 25.0, 100.0, 125.0, 200.0] {
                for phase in [0.0f32, 0.25, 0.5, 0.75] {
                    let mut transformed = path.clone();
                    transformed.transform(CfxMatrix::new(
                        size,
                        0.0,
                        0.0,
                        -size,
                        phase,
                        240.0 + phase,
                    ));
                    let right = transformed
                        .points()
                        .iter()
                        .map(|p| p.point.x)
                        .fold(f32::NEG_INFINITY, f32::max);
                    let top = transformed
                        .points()
                        .iter()
                        .map(|p| p.point.y)
                        .fold(f32::INFINITY, f32::min);
                    let precision = f32::EPSILON * 240.0 * 2.0;
                    assert!((right - (phase + width_units * size / 1000.0)).abs() <= precision);
                    assert!((top - ((240.0 + phase) - 700.0 * size / 1000.0)).abs() <= precision);
                    // The observed regression is the exact700-unit top edge
                    // at100/200pt: reciprocal multiplication expanded this
                    // rectangle upwards by a complete solid pixel row.
                    if size == 100.0 || size == 200.0 {
                        let bounds = transformed.rect(None).unwrap().get_outer_rect();
                        assert_eq!(bounds.top, ((240.0 + phase) - size * 0.7).floor() as i32);
                        assert_eq!(bounds.bottom, (240.0 + phase).ceil() as i32);
                    }
                }
            }
        }
    }

    #[test]
    fn hint_reliant_paths_and_bitmaps_share_fixed_mono_outlines() {
        // Original synthetic font: the right edge starts at 400 font units.
        // SHPIX moves it by 200 pixels. Only mono hinting at 1000 ppem
        // reconstructs the intended 600-unit width, independent of render size.
        let face =
            CfxFace::new(include_bytes!("../../tests/fixtures/hint-reliant.ttf"), 0).unwrap();
        assert!(face.is_tricky());
        let glyph = face.get_char_index(u32::from(b'A'));
        let path = face.load_glyph_path(glyph, 0, false, None).unwrap();
        let right = path.points().iter().map(|p| p.point.x).fold(0.0, f32::max);
        assert!((right - 0.6).abs() < 0.000001);
        assert_eq!(face.glyph_bbox(glyph), FxRect::new(0, 700, 600, 0));
        assert_eq!(
            face.font_glyph_bbox(glyph),
            Some(FxRect::new(0, 0, 600, 700))
        );
        assert_eq!(face.char_bbox(glyph as i32).right, 600);
        for size in [10.0, 13.5, 25.0, 1000.0] {
            let glyph_bitmap = face
                .render_glyph(
                    glyph,
                    false,
                    false,
                    CfxMatrix::new(size, 0.0, 0.0, size, 0.0, 0.0),
                    0,
                    FontAntiAliasingMode::Normal,
                    None,
                )
                .unwrap();
            let bitmap = glyph_bitmap.bitmap();
            assert_eq!(bitmap.format(), FxdibFormat::EightBppMask);
            let x = (size * 0.5) as usize;
            let y = (size * 0.35) as i32;
            assert!(bitmap.width() as usize > x);
            assert!(bitmap.scanline(y).unwrap()[x] > 192);
            if size == 13.5 {
                assert!(bitmap.buffer().iter().any(|&v| v > 0 && v < 255));
            }
            let hinter = face.hinter.borrow();
            let instance = hinter.as_ref().unwrap().as_ref().unwrap();
            assert_eq!(instance.size(), Size::new(1000.0));
            assert_eq!(instance.target(), skrifa::outline::Target::Mono);
            drop(hinter);
            assert_eq!(
                face.load_glyph_path(glyph, 0, false, None)
                    .unwrap()
                    .points(),
                path.points()
            );
        }
        assert!(face.load_glyph_path(u32::MAX, 0, false, None).is_none());
        // A required interpreter that failed initialization must not silently
        // return an unhinted, structurally incorrect glyph.
        *face.hinter.borrow_mut() = Some(None);
        assert!(face.load_glyph_path(glyph, 0, false, None).is_none());
        assert!(face
            .render_glyph(
                glyph,
                false,
                false,
                CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0),
                0,
                FontAntiAliasingMode::Normal,
                None,
            )
            .is_none());
    }

    #[test]
    fn reused_glyph_scratch_matches_fresh_faces_and_keeps_output_independent() {
        fn snapshot(glyph: &CfxGlyphBitmap) -> (CfxPoint, FxdibFormat, i32, i32, u32, Vec<u8>) {
            let bitmap = glyph.bitmap();
            (
                glyph.top_left_origin(),
                bitmap.format(),
                bitmap.width(),
                bitmap.height(),
                bitmap.pitch(),
                bitmap.buffer().to_vec(),
            )
        }

        for data in [
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf")
                .as_slice(),
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/roboto.ttf")
                .as_slice(),
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/times_bold.ttf")
                .as_slice(),
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/foxit_fixed.cff")
                .as_slice(),
            include_bytes!("../../tests/fixtures/triangle.pfa").as_slice(),
        ] {
            let face = CfxFace::new(data, 0).unwrap();
            let mut retained = Vec::new();
            for pass in 0..2 {
                for (character, size, mode, weight) in [
                    ('A' as u32, 48.0, FontAntiAliasingMode::Lcd, 700),
                    ('g' as u32, 9.0, FontAntiAliasingMode::Mono, 400),
                    (u32::MAX, 24.0, FontAntiAliasingMode::Normal, 700),
                    ('A' as u32, 10_000.0, FontAntiAliasingMode::Lcd, 400),
                    (' ' as u32, 24.0, FontAntiAliasingMode::Normal, 400),
                    ('A' as u32, 0.0, FontAntiAliasingMode::Normal, 400),
                    ('A' as u32, 13.5, FontAntiAliasingMode::Normal, 400),
                    ('g' as u32, -18.0, FontAntiAliasingMode::Lcd, 700),
                ] {
                    let fresh = CfxFace::new(data, 0).unwrap();
                    let glyph = if character == u32::MAX {
                        u32::MAX
                    } else {
                        face.get_char_index(character)
                    };
                    let matrix = CfxMatrix::new(size, size * 0.1, 0.0, size, 0.0, 0.0);
                    let mut subst = CfxSubstFont::new();
                    subst.set_weight(weight);
                    subst.set_italic_angle(-12);
                    let render = |face: &CfxFace<'_>| {
                        face.render_glyph(glyph, false, pass != 0, matrix, 0, mode, Some(&subst))
                    };
                    let expected = render(&fresh);
                    let actual = render(&face);
                    assert_eq!(
                        actual.as_ref().map(snapshot),
                        expected.as_ref().map(snapshot),
                        "format={}, pass={pass}, character={character}, size={size}, mode={mode:?}",
                        face.font_format(),
                    );
                    if let Some(actual) = actual {
                        let expected = snapshot(&actual);
                        retained.push((actual, expected));
                    }
                }
            }
            // Later larger/smaller glyphs and failing renders must not mutate
            // any bitmap already returned to the glyph cache.
            for (glyph, expected) in retained {
                assert_eq!(snapshot(&glyph), expected);
            }
        }
    }

    #[test]
    fn quadratic_outline_converts_to_pdfium_bezier_points() {
        let mut pen = GlyphPathPen::new();
        pen.move_to(0.0, 0.0);
        pen.quad_to(3.0, 6.0, 6.0, 0.0);
        pen.close();
        let points = pen.path.points();
        assert_eq!(points.len(), 4);
        assert_eq!(points[1].point, CfxPointF::new(2.0, 4.0));
        assert_eq!(points[2].point, CfxPointF::new(4.0, 4.0));
        assert_eq!(points[3].point, CfxPointF::new(6.0, 0.0));
        assert!(points[3].close_figure);
    }

    #[test]
    fn expanded_bold_preserves_anchor_and_counters_for_both_windings_and_lcd() {
        use kurbo::Shape;

        for reverse in [false, true] {
            for x_scale in [1.0, 3.0] {
                let mut pen = GlyphPathPen::new();
                for (lo, hi, backwards) in [(0.0, 10.0, reverse), (3.0, 7.0, !reverse)] {
                    let mut corners = [(lo, lo), (hi, lo), (hi, hi), (lo, hi)];
                    if backwards {
                        corners.reverse();
                    }
                    pen.move_to(corners[0].0, corners[0].1);
                    for (x, y) in &corners[1..] {
                        pen.line_to(*x, *y);
                    }
                    pen.close();
                }
                let mut input = Vec::new();
                expand_glyph_outline(
                    &mut pen,
                    CfxMatrix::new(x_scale, 0.0, 0.0, 1.0, 0.0, 0.0),
                    128, // Two device pixels of total stem growth.
                    x_scale,
                    &mut input,
                )
                .unwrap();
                let mut shape = Vec::new();
                for p in pen.path.points() {
                    let point = Point::new(f64::from(p.point.x), f64::from(p.point.y));
                    shape.push(match p.point_type {
                        PointType::Move => PathEl::MoveTo(point),
                        PointType::Line => PathEl::LineTo(point),
                        PointType::Bezier => panic!("rectilinear miter expansion stays linear"),
                    });
                    if p.close_figure {
                        shape.push(PathEl::ClosePath);
                    }
                }
                let shape = shape.as_slice();
                // Native placement keeps the left/bottom edges fixed, grows
                // right/up, and shrinks the counter instead of painting it in.
                assert_eq!(
                    shape.bounding_box(),
                    kurbo::Rect::new(0.0, -2.0, 12.0 * f64::from(x_scale), 10.0)
                );
                // Offset joins may overlap. Test the actual nonzero fill,
                // rather than the sum of signed contour areas.
                for y in -3..11 {
                    for x in -1..13 * x_scale as i32 {
                        let point = Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
                        let outer = (0.0..12.0 * f64::from(x_scale)).contains(&point.x)
                            && (-2.0..10.0).contains(&point.y);
                        let counter = (5.0 * f64::from(x_scale)..7.0 * f64::from(x_scale))
                            .contains(&point.x)
                            && (3.0..5.0).contains(&point.y);
                        assert_eq!(shape.winding(point) != 0, outer && !counter);
                    }
                }
            }
        }
    }

    #[test]
    fn expanded_bold_scratch_is_accounted_and_trimmed_without_pixel_changes() {
        use super::super::font_memory::FontMemoryLimits;

        let memory = Rc::new(FontMemory::default());
        let face = CfxFace::new_with_memory(
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"),
            0,
            memory.clone(),
        )
        .unwrap();
        let glyph = face.unicode_char_index(u32::from(b'A'));
        let mut subst = CfxSubstFont::new();
        subst.set_weight(700);
        let draw = || {
            face.render_glyph(
                glyph,
                false,
                false,
                CfxMatrix::new(25.0, 0.0, 0.0, 25.0, 0.0, 0.0),
                0,
                FontAntiAliasingMode::Normal,
                Some(&subst),
            )
            .unwrap()
        };
        let image = draw();
        let retained = memory.usage().glyph_scratch_bytes;
        let scratch = face.render_scratch.checkout();
        let input_bytes = scratch.expand_input.capacity() * std::mem::size_of::<PathEl>();
        assert!(input_bytes > 0);
        assert!(retained >= input_bytes);
        assert_eq!(retained, scratch.retained_bytes());
        drop(scratch);
        assert_eq!(memory.usage().glyph_scratch_bytes, retained);
        memory.set_limits(FontMemoryLimits {
            glyph_scratch_bytes: 0,
            ..FontMemoryLimits::default()
        });
        assert_eq!(memory.usage().glyph_scratch_bytes, 0);
        let uncached = draw();
        assert_eq!(memory.usage().glyph_scratch_bytes, 0);
        assert_eq!(memory.usage().active_scratch, 0);
        assert_eq!(uncached.top_left_origin(), image.top_left_origin());
        assert_eq!(uncached.bitmap().buffer(), image.bitmap().buffer());
    }

    #[test]
    fn sfnt_face_reads_metrics_and_unicode_cmap() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let face = CfxFace::new(data, 0).unwrap();
        assert!(face.is_tt_ot());
        assert_eq!(face.font_format(), "TrueType");
        assert_eq!(face.family_name(), "Ahem");
        assert!(face.units_per_em() > 0);
        assert!(face.glyph_count() > 100);
        assert_ne!(face.unicode_char_index(u32::from('A')), 0);
        let a_glyph = face.unicode_char_index(u32::from('A'));
        assert!(face.glyph_tt_width(a_glyph) > 0);
        assert!(face.glyph_width(a_glyph, 0, None) > 0);
        let mut subst = CfxSubstFont::new();
        subst.set_is_builtin_generic_font();
        assert_eq!(face.glyph_width(a_glyph, 321, Some(&subst)), 321);
        let bbox = face.glyph_bbox(a_glyph);
        assert_eq!(
            face.font_glyph_bbox(a_glyph),
            Some(FxRect::new(bbox.left, bbox.bottom, bbox.right, bbox.top))
        );
        assert_eq!(face.font_glyph_bbox(face.glyph_count()), None);
        assert_eq!(face.char_bbox(-1), FxRect::default());
        assert_eq!(face.char_bbox(a_glyph as i32).top, bbox.top + bbox.top / 64);
        let path = face.load_glyph_path(a_glyph, 0, false, None).unwrap();
        assert!(!path.points().is_empty());
        assert!(path.points().iter().all(|point| point.point.x.abs() < 2.0));
        assert!(face
            .load_glyph_path(face.glyph_count(), 0, false, None)
            .is_none());
        assert_eq!(face.current_charmap_encoding(), FontEncoding::Unicode);
        assert!(face.charmap_count() > 0);
        assert_eq!(
            face.get_char_index(u32::from('A')),
            face.unicode_char_index(u32::from('A'))
        );
        assert!(face.select_charmap(FontEncoding::Unicode));
        assert!(face.current_charmap().is_some());
        assert_eq!(
            face.get_char_index(u32::from('A')),
            face.unicode_char_index(u32::from('A'))
        );
        assert!(face.set_charmap(None));
        assert_eq!(face.current_charmap(), None);
        let codes = face.get_char_codes_and_indices(0xffff);
        assert_eq!(codes.len(), 278);
        assert_eq!(
            codes[0],
            CharCodeAndIndex {
                char_code: 32,
                glyph_index: 3
            }
        );
        assert_eq!(
            codes[1],
            CharCodeAndIndex {
                char_code: 33,
                glyph_index: 4
            }
        );
        assert_eq!(
            codes[277],
            CharCodeAndIndex {
                char_code: 65279,
                glyph_index: 245
            }
        );
        assert_eq!(face.get_char_codes_and_indices(32), codes[..1]);
        assert_eq!(face.data(), data);
        assert_eq!(
            CfxFace::new(b"bad font", 0).err(),
            Some(CfxFaceError::InvalidFont)
        );
        let name_len = face.get_sfnt_table(u32::from_be_bytes(*b"name"), &mut []);
        assert!(name_len > 0);
        let mut name_table = vec![0; name_len];
        assert_eq!(
            face.get_sfnt_table(u32::from_be_bytes(*b"name"), &mut name_table),
            name_len
        );
        assert_eq!(
            face.get_sfnt_table(u32::from_be_bytes(*b"name"), &mut name_table[..1]),
            0
        );
        assert_eq!(
            face.get_sfnt_table(u32::from_be_bytes(*b"xxxx"), &mut []),
            0
        );
    }
}

#[cfg(test)]
#[path = "cfx_face_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "skrifa/fx_skrifa_unittest.rs"]
mod skrifa_upstream_tests;

#[cfg(test)]
#[path = "cfx_face_charmap_enumeration_tests.rs"]
mod charmap_enumeration_tests;
