// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! PDF sample interpretation from `core/fpdfapi/page/cpdf_dib.{h,cpp}`.
//! LoadColorInfo, decode/color-key arrays, palettes and scanline translation are
//! connected for raw, Flate, RunLength, JPEG/JPX and packed CCITT/JBIG2 samples.
//! Retained decoder rows and parent/mask continuation connect to image/cache
//! continuation; codec-specific behavior stays behind the planned Rust codecs.

use crate::fxcodec::jbig2::jbig2_document_context::Jbig2DocumentContext;
use crate::fxcrt::fx_coordinates::CfxSize;
use std::cell::{RefCell, RefMut};
use std::rc::{Rc, Weak};

use super::cpdf_colorspace::{get_stock_device_cs, ColorSpaceFamily, ColorSpaceRef};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfRetainedStreamAcc;
use crate::fpdfapi::parser::fpdf_parser_decode::{create_flate_decoder, get_decoder_array};
use crate::fxcodec::scanlinedecoder::ScanlineDecoder;
use crate::fxge::calculate_pitch::{calculate_pitch32, calculate_pitch8};
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::cstretchengine::StretchSource;
use crate::fxge::dib::fx_dib::{get_bpp_from_format, FxRgbStruct, FxdibFormat};

#[derive(Default)]
struct DibCompData {
    decode_min: f32,
    decode_step: f32,
    color_key_min: u32,
    color_key_max: u32,
}

#[derive(Default)]
struct ScanBuffers {
    line: Vec<u8>,
    mask: Vec<u8>,
    remainder: Vec<u8>,
    history: Option<ScanlineHistory>,
}

// Bilinear tiles can revisit the preceding two rows. Retain those converted
// rows alongside the current output buffer, rather than rewinding a streaming
// decoder to the middle of a large image. Buffers rotate without copying rows.
// Only lazy page-cache Flate images opt in; ordinary/realized images retain the
// original one-row path. The two additional buffers have a fixed memory cap.
const SCANLINE_HISTORY_BYTES: usize = 256 * 1024;

struct RetainedScanline {
    position: Option<(i32, usize)>,
    pixels: Vec<u8>,
}

struct ScanlineHistory {
    current: Option<(i32, usize)>,
    previous: [RetainedScanline; 2],
    next: usize,
}

impl ScanlineHistory {
    fn new(pitch: usize) -> Option<Self> {
        if pitch == 0 || pitch.checked_mul(2)? > SCANLINE_HISTORY_BYTES {
            return None;
        }
        let make_row = || {
            let mut pixels = Vec::new();
            pixels.try_reserve_exact(pitch).ok()?;
            pixels.resize(pitch, 0);
            Some(RetainedScanline {
                position: None,
                pixels,
            })
        };
        Some(Self {
            current: None,
            previous: [make_row()?, make_row()?],
            next: 0,
        })
    }

    fn lookup(&self, row: i32) -> Option<(Option<usize>, usize)> {
        if let Some((position, len)) = self.current {
            if position == row {
                return Some((None, len));
            }
        }
        self.previous.iter().enumerate().find_map(|(index, entry)| {
            entry
                .position
                .filter(|&(position, _)| position == row)
                .map(|(_, len)| (Some(index), len))
        })
    }

    fn clear(&mut self) {
        self.current = None;
        for row in &mut self.previous {
            row.position = None;
        }
    }

    fn retained_bytes(&self) -> usize {
        self.previous.iter().map(|row| row.pixels.capacity()).sum()
    }
}

impl ScanBuffers {
    fn prepare_scanline(&mut self, mask: bool) {
        let Some(history) = &mut self.history else {
            return;
        };
        let Some(position) = history.current.take() else {
            return;
        };
        let entry = &mut history.previous[history.next];
        entry.position = Some(position);
        std::mem::swap(
            if mask { &mut self.mask } else { &mut self.line },
            &mut entry.pixels,
        );
        history.next = (history.next + 1) % history.previous.len();
    }
}

pub struct DibScanline<'a>(DibScanlineStorage<'a>);
enum DibScanlineStorage<'a> {
    Buffer(RefMut<'a, [u8]>),
    Cached(&'a [u8]),
}
impl std::ops::Deref for DibScanline<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match &self.0 {
            DibScanlineStorage::Buffer(row) => row,
            DibScanlineStorage::Cached(row) => row,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadState {
    Fail,
    Success,
    Continue,
}

pub struct CpdfDib {
    jbig_decode_job: Option<crate::fxcodec::jbig2::jbig2_decoder::Jbig2DecodeJob>,
    codec_context: Weak<RefCell<Jbig2DocumentContext>>,
    // Rust drops fields in declaration order: the decoder releases its source
    // before the stream accessor, matching native member destruction order.
    scanline_decoder: RefCell<Option<Box<dyn ScanlineDecoder>>>,
    stream_acc: CpdfRetainedStreamAcc,
    resolution_levels_to_skip: u8,
    // Original normalized hint, before JPX's supported-level fallback changes it.
    cache_resolution_request: u8,
    cached_bitmap: Option<CfxDibitmap>,
    jbig_state: LoadState,
    jbig_started: bool,
    global_acc: Option<CpdfRetainedStreamAcc>,
    inline_mask: RefCell<Option<CfxDibitmap>>,
    external_mask: RefCell<Option<Box<CpdfDib>>>,
    mask_pending: bool,
    has_mask: bool,
    matte_color: u32,
    std_cs: bool,
    group_family: ColorSpaceFamily,
    load_mask: bool,
    width: i32,
    height: i32,
    pitch: u32,
    source_pitch: usize,
    format: FxdibFormat,
    color_space: Option<ColorSpaceRef>,
    // Preserve the actual decoder input before JPX can adopt another output
    // space/size. The cache must not infer source detail from dictionary sizes.
    cache_input_color_space: Option<ColorSpaceRef>,
    cache_native_size: CfxSize,
    cache_size_request_set: bool,
    cache_conversion_revision: Option<u64>,
    family: ColorSpaceFamily,
    bpc: u32,
    components: u32,
    default_decode: bool,
    image_mask: bool,
    color_key: bool,
    comp_data: Vec<DibCompData>,
    palette: Vec<u32>,
    buffers: RefCell<ScanBuffers>,
    scanline_history_bytes: usize,
}

fn get_bits(data: &[u8], bit: usize, bits: u32) -> u32 {
    let byte = u32::from(data[bit / 8]);
    match bits {
        8 => byte,
        16 => byte * 256 + u32::from(data[bit / 8 + 1]),
        _ => (byte >> (8 - bits - (bit % 8) as u32)) & ((1 << bits) - 1),
    }
}

fn rgb_argb(rgb: FxRgbStruct<f32>) -> u32 {
    0xff00_0000
        | ((rgb.red * 255.0).round() as i32 as u32).wrapping_shl(16)
        | ((rgb.green * 255.0).round() as i32 as u32).wrapping_shl(8)
        | (rgb.blue * 255.0).round() as i32 as u32
}

impl CpdfDib {
    pub(super) fn retain_scanline_history(&mut self) {
        if self.cached_bitmap.is_none()
            && self.scanline_decoder.get_mut().is_some()
            && crate::fxcrt::bytestring::eq_cstr(
                self.stream_acc.get_image_decoder(),
                b"FlateDecode",
            )
            && self.buffers.get_mut().history.is_none()
        {
            self.buffers.get_mut().history = ScanlineHistory::new(self.pitch as usize);
            self.scanline_history_bytes = self
                .buffers
                .get_mut()
                .history
                .as_ref()
                .map_or(0, ScanlineHistory::retained_bytes);
        }
    }

    pub fn color_space(&self) -> Option<ColorSpaceRef> {
        self.color_space.as_ref().map(Rc::clone)
    }
    pub fn components(&self) -> u32 {
        self.components
    }
    pub fn load(stream: ObjectRef, color_space: Option<ColorSpaceRef>) -> Option<Self> {
        Self::load_with_max_size(stream, color_space, CfxSize::new(0, 0))
    }
    pub fn load_with_max_size(
        stream: ObjectRef,
        color_space: Option<ColorSpaceRef>,
        max_size: CfxSize,
    ) -> Option<Self> {
        let data = super::cpdf_docpagedata::CpdfDocPageData::new();
        let mut context = super::cpdf_image::CpdfImageLoadContext::new(&data, None);
        context.max_size = max_size;
        Self::load_with_context(stream, color_space, context, false)
    }

    pub fn load_with_context(
        stream: ObjectRef,
        color_space: Option<ColorSpaceRef>,
        context: super::cpdf_image::CpdfImageLoadContext<'_>,
        has_mask: bool,
    ) -> Option<Self> {
        let (mut source, mut state) =
            Self::start_load_with_context(stream, color_space, context, has_mask)?;
        while state == LoadState::Continue {
            state = source.continue_load(None);
        }
        (state == LoadState::Success).then_some(source)
    }

    pub fn start_load_with_max_size(
        stream: ObjectRef,
        color_space: Option<ColorSpaceRef>,
        max_size: CfxSize,
    ) -> Option<(Self, LoadState)> {
        let data = super::cpdf_docpagedata::CpdfDocPageData::new();
        let mut context = super::cpdf_image::CpdfImageLoadContext::new(&data, None);
        context.max_size = max_size;
        Self::start_load_with_context(stream, color_space, context, false)
    }

    pub fn start_load_with_context(
        stream: ObjectRef,
        color_space: Option<ColorSpaceRef>,
        context: super::cpdf_image::CpdfImageLoadContext<'_>,
        has_mask: bool,
    ) -> Option<(Self, LoadState)> {
        let max_size = context.max_size;
        let object = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()?
            .get_dict();
        let borrowed = object.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        let width = dict.get_integer_for(b"Width", 0);
        let height = dict.get_integer_for(b"Height", 0);
        if !(1..=0x01ffff).contains(&width) || !(1..=0x01ffff).contains(&height) {
            return None;
        }
        let decoders = get_decoder_array(dict)?;
        let filter = decoders
            .last()
            .map(|(name, _)| crate::fxcrt::bytestring::c_str(name))
            .unwrap_or_default();
        let bilevel = matches!(filter, b"CCITTFaxDecode" | b"JBIG2Decode");
        let original_bpc = dict.get_integer_for(b"BitsPerComponent", 0);
        if !(0..=16).contains(&original_bpc) {
            return None;
        }
        let is_jpx = filter == b"JPXDecode";
        let no_color_jpx =
            is_jpx && !dict.key_exist(b"ColorSpace") && !dict.get_boolean_for(b"ImageMask", false);
        let image_mask = dict.get_boolean_for(b"ImageMask", false)
            || (!dict.key_exist(b"ColorSpace") && !no_color_jpx);
        let (bpc, components, family) = if image_mask {
            (1, 1, ColorSpaceFamily::Unknown)
        } else if no_color_jpx {
            (0, 0, ColorSpaceFamily::Unknown)
        } else {
            let space = color_space.as_ref()?;
            let mut components = space.component_count();
            if space.family() == ColorSpaceFamily::IccBased {
                let name = dict.get_byte_string_for(b"ColorSpace", b"");
                components = match crate::fxcrt::bytestring::c_str(&name) {
                    b"DeviceGray" => 1,
                    b"DeviceRGB" => 3,
                    b"DeviceCMYK" => 4,
                    _ => components,
                };
            }
            let bpc = if bilevel {
                components = 1;
                1
            } else if filter == b"DCTDecode" {
                8
            } else {
                original_bpc
            };
            if !is_jpx && (!matches!(bpc, 1 | 2 | 4 | 8 | 16) || components == 0) {
                return None;
            }
            (bpc as u32, components, space.family())
        };
        let source_pitch = calculate_pitch8(bpc, components, width)?;
        let estimated_size = source_pitch.checked_mul(height as u32)?;
        let stream_acc = CpdfRetainedStreamAcc::load_image(stream, estimated_size)?;
        if stream_acc.get_span().is_empty() {
            return None;
        }
        let resolution_levels = cache_inputs::resolution_levels(width, height, max_size);
        let mut result = Self {
            jbig_decode_job: None,
            stream_acc,
            scanline_decoder: RefCell::new(None),
            resolution_levels_to_skip: resolution_levels,
            cache_resolution_request: cache_inputs::cache_resolution_levels(
                filter,
                resolution_levels,
            ),
            cached_bitmap: None,
            jbig_state: LoadState::Fail,
            jbig_started: false,
            codec_context: Weak::new(),
            global_acc: None,
            inline_mask: RefCell::new(None),
            external_mask: RefCell::new(None),
            mask_pending: false,
            has_mask,
            matte_color: 0,
            std_cs: context.std_cs,
            group_family: context.group_family,
            load_mask: context.load_mask,
            width,
            height,
            pitch: 0,
            source_pitch: source_pitch as usize,
            format: FxdibFormat::Invalid,
            cache_input_color_space: color_space.as_ref().map(Rc::clone),
            cache_native_size: CfxSize::new(width, height),
            cache_size_request_set: max_size.width != 0 && max_size.height != 0,
            cache_conversion_revision: super::cpdf_colorspace::cmyk_conversion_revision(),
            color_space: if image_mask { None } else { color_space },
            family,
            bpc,
            components,
            default_decode: true,
            image_mask,
            color_key: false,
            comp_data: Vec::new(),
            palette: Vec::new(),
            buffers: RefCell::new(ScanBuffers::default()),
            scanline_history_bytes: 0,
        };
        if image_mask {
            result.default_decode = dict.get_array_for(b"Decode").is_none_or(|object| {
                object
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .unwrap()
                    .get_integer_at(0)
                    == 0
            });
        } else if result.color_space.is_some() {
            result.get_decode_and_mask_array(dict);
        }
        result.create_decoder(dict)?;
        if result.std_cs {
            if let Some(space) = &result.color_space {
                space.enable_std_conversion(true);
            }
        }
        // ContinueInternal(): format, line storage and palette use the decoder's
        // adopted dimensions and components.
        if result.image_mask {
            result.bpc = 1;
            result.components = 1;
        }
        let bits = result.bpc.checked_mul(result.components)?;
        if bits == 0 {
            return None;
        }
        result.format = if result.image_mask {
            FxdibFormat::OneBppMask
        } else if bits == 1 {
            FxdibFormat::OneBppRgb
        } else if bits <= 8 {
            FxdibFormat::EightBppRgb
        } else {
            FxdibFormat::Bgr
        };
        result.pitch = calculate_pitch32(get_bpp_from_format(result.format) as i32, result.width)?;
        result.source_pitch =
            calculate_pitch8(result.bpc, result.components, result.width)? as usize;
        result.buffers.get_mut().line = vec![0; result.pitch as usize];
        result.load_palette();
        if result.color_key {
            result.format = FxdibFormat::Bgra;
            result.pitch = calculate_pitch32(32, result.width)?;
            result.buffers.get_mut().mask = vec![0; result.pitch as usize];
        }
        if result.jbig_state == LoadState::Continue {
            result.codec_context = Rc::downgrade(&context.page_data.get_or_create_codec_context());
        }
        let mask_state = if has_mask {
            result.start_load_mask(dict, context)
        } else {
            LoadState::Success
        };
        let state = if result.jbig_state == LoadState::Continue || mask_state == LoadState::Continue
        {
            LoadState::Continue
        } else {
            LoadState::Success
        };
        if state == LoadState::Success {
            result.disable_std_conversion();
        }
        Some((result, state))
    }

    pub fn continue_load(
        &mut self,
        mut pause: Option<&mut dyn crate::fxcrt::pauseindicator_iface::PauseIndicatorIface>,
    ) -> LoadState {
        if self.mask_pending {
            return self.continue_load_mask(pause);
        }
        if crate::fxcrt::bytestring::eq_cstr(self.stream_acc.get_image_decoder(), b"JPXDecode") {
            return LoadState::Fail;
        }
        if !crate::fxcrt::bytestring::eq_cstr(self.stream_acc.get_image_decoder(), b"JBIG2Decode") {
            return LoadState::Success;
        }
        if self.jbig_state != LoadState::Continue {
            return self.jbig_state;
        }
        if !self.jbig_started {
            self.jbig_started = true;
            let param = self.stream_acc.get_image_param();
            let globals = param.and_then(|object| {
                object
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()?
                    .get_stream_for(b"JBIG2Globals")
            });
            self.global_acc = globals.and_then(CpdfRetainedStreamAcc::load_filtered);
        }
        use crate::fxcodec::jbig2::jbig2_decoder::{DecodeStatus, Jbig2DecodeJob};
        if self.jbig_decode_job.is_none() {
            self.jbig_decode_job = Jbig2DecodeJob::new(self.width as u32, self.height as u32);
        }
        let globals = self
            .global_acc
            .as_ref()
            .map(CpdfRetainedStreamAcc::get_span);
        let status = match (&mut self.jbig_decode_job, &mut self.cached_bitmap) {
            (Some(job), Some(bitmap)) => job.continue_decode(
                &self.stream_acc.get_span(),
                globals.as_deref(),
                bitmap,
                &self.codec_context,
                self.global_acc
                    .as_ref()
                    .map_or(0, CpdfRetainedStreamAcc::key_for_cache),
                match pause.as_mut() {
                    Some(owner) => Some(&mut **owner),
                    None => None,
                },
            ),
            _ => DecodeStatus::Error,
        };
        drop(globals);
        if status == DecodeStatus::Continue {
            return LoadState::Continue;
        }
        self.jbig_decode_job = None;
        let success = status == DecodeStatus::Finished;
        self.jbig_state = if success {
            LoadState::Success
        } else {
            LoadState::Fail
        };
        if !success {
            self.cached_bitmap = None;
            self.global_acc = None;
            self.jbig_started = false;
            return LoadState::Fail;
        }
        if self.has_mask && self.continue_load_mask(pause) == LoadState::Continue {
            self.mask_pending = true;
            return LoadState::Continue;
        }
        // A completed parent JBIG2 decoder only propagates a pending mask;
        // a mask failure here discards that mask and retains the parent image.
        self.disable_std_conversion();
        LoadState::Success
    }

    fn create_decoder(&mut self, dict: &CpdfDictionary) -> Option<()> {
        let decoder = self.stream_acc.get_image_decoder().to_vec();
        let param = self.stream_acc.get_image_param();
        let borrowed = param.as_ref().map(|param| param.borrow());
        let params = borrowed
            .as_ref()
            .and_then(|param| param.as_any().downcast_ref::<CpdfDictionary>());
        match crate::fxcrt::bytestring::c_str(&decoder) {
            b"" if decoder.is_empty() => {}
            b"FlateDecode" => {
                let source = self.stream_acc.share_data();
                let decoder = create_flate_decoder(
                    &source,
                    self.width,
                    self.height,
                    self.components as i32,
                    self.bpc as i32,
                    params,
                )?;
                *self.scanline_decoder.get_mut() =
                    Some(Box::new(decoder.retain_source(Rc::clone(&source))));
            }
            b"RunLengthDecode" => {
                *self.scanline_decoder.get_mut() = Some(crate::fxcodec::basic::basicmodule::BasicModule::create_run_length_decoder_shared(
                    self.stream_acc.share_data(), self.width, self.height,
                    self.components as i32, self.bpc as i32,
                )?);
            }
            b"CCITTFaxDecode" => {
                let source = self.stream_acc.share_data();
                let decoder = crate::fpdfapi::parser::fpdf_parser_decode::create_fax_decoder(
                    &source,
                    self.width,
                    self.height,
                    params,
                )?;
                *self.scanline_decoder.get_mut() =
                    Some(Box::new(decoder.retain_source(Rc::clone(&source))));
            }
            b"JBIG2Decode" => {
                self.cached_bitmap = Some(CfxDibitmap::create(
                    self.width,
                    self.height,
                    if self.image_mask {
                        FxdibFormat::OneBppMask
                    } else {
                        FxdibFormat::OneBppRgb
                    },
                    0,
                )?);
                self.jbig_state = LoadState::Continue;
            }
            b"DCTDecode" => self.create_dct_decoder(dict, params)?,
            b"JPXDecode" => self.load_jpx_bitmap(dict)?,
            _ => return None,
        }
        if let Some(decoder) = self.scanline_decoder.get_mut().as_ref() {
            let requested = calculate_pitch8(self.bpc, self.components, self.width)?;
            let provided = calculate_pitch8(
                decoder.get_bpc() as u32,
                decoder.count_comps() as u32,
                decoder.get_width(),
            )?;
            if provided < requested {
                return None;
            }
        }
        Some(())
    }

    fn create_dct_decoder(
        &mut self,
        dict: &CpdfDictionary,
        params: Option<&CpdfDictionary>,
    ) -> Option<()> {
        let scale_denom = 1u32 << self.resolution_levels_to_skip.min(3);
        let color_transform =
            params.is_none_or(|params| params.get_integer_for(b"ColorTransform", 1) != 0);
        let first = crate::fxcodec::jpeg::decode_with_validation(
            &self.stream_acc.get_span(),
            self.width,
            self.height,
            self.components as i32,
            color_transform,
            scale_denom,
        );
        let decoded = if let Some(decoded) = first {
            decoded
        } else {
            let info = crate::fxcodec::jpeg::load_info(&self.stream_acc.get_span())?;
            self.width = info.width;
            self.height = info.height;
            if !matches!(info.components, 1 | 3 | 4) {
                return None;
            }
            if self.components != info.components {
                self.components = info.components;
                self.comp_data.clear();
                if let Some(space) = &self.color_space {
                    let count = space.component_count();
                    let valid = match self.family {
                        ColorSpaceFamily::DeviceGray => count >= 1 && self.components >= 1,
                        ColorSpaceFamily::DeviceRgb => count >= 3 && self.components >= 3,
                        ColorSpaceFamily::DeviceCmyk => count >= 4 && self.components >= 4,
                        ColorSpaceFamily::Lab => self.components == 3 && count >= 3,
                        ColorSpaceFamily::IccBased => {
                            matches!(count, 1 | 3 | 4)
                                && matches!(self.components, 1 | 3 | 4)
                                && count >= self.components
                        }
                        _ => count == self.components,
                    };
                    if !valid {
                        return None;
                    }
                } else if self.family == ColorSpaceFamily::Lab && self.components != 3 {
                    return None;
                }
                // Native GetDecodeAndMaskArray returns false without a color space.
                self.color_space.as_ref()?;
                self.get_decode_and_mask_array(dict);
            }
            self.bpc = 8;
            crate::fxcodec::jpeg::decode_with_validation(
                &self.stream_acc.get_span(),
                self.width,
                self.height,
                self.components as i32,
                info.color_transform,
                scale_denom,
            )?
        };
        self.width = decoded.width;
        self.height = decoded.height;
        *self.scanline_decoder.get_mut() = Some(Box::new(
            crate::fxcodec::jpeg::JpegScanlineDecoder::from_decoded(decoded)?,
        ));
        Some(())
    }

    fn load_jpx_bitmap(&mut self, dict: &CpdfDictionary) -> Option<()> {
        use super::jpx_decode_conversion::JpxDecodeConversion;
        use crate::fxcodec::jpx::CjpxDecoder;
        let source = self.stream_acc.get_span();
        let indexed = self
            .color_space
            .as_ref()
            .is_some_and(|space| space.family() == ColorSpaceFamily::Indexed);
        let (decoder, actual_skip) = CjpxDecoder::create_with_resolution_hint(
            &source,
            indexed,
            self.resolution_levels_to_skip,
        )?;
        self.resolution_levels_to_skip = actual_skip;
        self.width = self
            .width
            .checked_shr(u32::from(self.resolution_levels_to_skip))
            .unwrap_or(0);
        self.height = self
            .height
            .checked_shr(u32::from(self.resolution_levels_to_skip))
            .unwrap_or(0);
        let info = decoder.get_info();
        if info.width < self.width as u32 || info.height < self.height as u32 {
            return None;
        }
        let conversion = JpxDecodeConversion::create(info, self.color_space.as_ref())?;
        if let Some(space) = conversion.override_colorspace() {
            self.color_space = space.clone();
        }
        if let Some(components) = conversion.jpx_components_count() {
            debug_assert_eq!(self.components, 0);
            self.components = components;
            if self.components == 0 {
                return None;
            }
        }
        // PDFium only detaches embedded RGB alpha (crbug.com/42270756).
        // A grayscale JPX can carry the same unassociated coverage plane;
        // decode both components before the ordinary gray conversion drops it.
        if info.colorspace == crate::fxcodec::jpx::JpxColorSpace::Gray
            && info.channels == 2
            && conversion.format() == FxdibFormat::EightBppRgb
            && dict.get_integer_for(b"SMaskInData", 0) == 1
        {
            let width = i32::try_from(info.width).ok()?;
            let height = i32::try_from(info.height).ok()?;
            let mut interleaved = CfxDibitmap::create(width, height, FxdibFormat::Bgr, 0)?;
            if !decoder.decode(&mut interleaved, false, 2) {
                return None;
            }
            let mut gray = CfxDibitmap::create(width, height, FxdibFormat::EightBppRgb, 0)?;
            let mut mask = CfxDibitmap::create(width, height, FxdibFormat::EightBppRgb, 0)?;
            for row in 0..height {
                let source = interleaved.scanline(row)?;
                let target = gray.scanline_mut(row)?;
                let coverage = mask.scanline_mut(row)?;
                for column in 0..info.width as usize {
                    target[column] = source[column * 2];
                    coverage[column] = source[column * 2 + 1];
                }
            }
            *self.inline_mask.get_mut() = Some(mask);
            self.bpc = 8;
            self.cached_bitmap = Some(gray);
            return Some(());
        }
        // SMaskInData 1 also permits an unassociated opacity component after
        // CMYK. Native's RGB-only extraction drops this fifth plane. Keep the
        // four color samples intact for the existing CMYK conversion, and
        // detach coverage exactly as for grayscale above.
        if info.colorspace == crate::fxcodec::jpx::JpxColorSpace::Cmyk
            && info.channels == 5
            && self.components == 4
            && self
                .color_space
                .as_ref()
                .is_some_and(|space| space.family() == ColorSpaceFamily::DeviceCmyk)
            && dict.get_integer_for(b"SMaskInData", 0) == 1
        {
            let width = i32::try_from(info.width).ok()?;
            let height = i32::try_from(info.height).ok()?;
            let columns = usize::try_from(info.width).ok()?;
            let source_bytes = columns.checked_mul(5)?;
            let mut bitmap = CfxDibitmap::create(
                conversion.width().try_into().ok()?,
                height,
                conversion.format(),
                0,
            )?;
            if !decoder.decode(&mut bitmap, false, 5) {
                return None;
            }
            let mut mask = CfxDibitmap::create(width, height, FxdibFormat::EightBppRgb, 0)?;
            for row in 0..height {
                let samples = bitmap.scanline_mut(row)?.get_mut(..source_bytes)?;
                let coverage = mask.scanline_mut(row)?;
                for column in 0..columns {
                    coverage[column] = samples[column * 5 + 4];
                    // Forward compaction never overwrites a following pixel.
                    samples.copy_within(column * 5..column * 5 + 4, column * 4);
                }
            }
            *self.inline_mask.get_mut() = Some(mask);
            self.bpc = 8;
            self.cached_bitmap = Some(bitmap);
            return Some(());
        }
        let mut bitmap = CfxDibitmap::create(
            conversion.width().try_into().ok()?,
            info.height.try_into().ok()?,
            conversion.format(),
            0,
        )?;
        if !decoder.decode(&mut bitmap, conversion.swap_rgb(), self.components) {
            return None;
        }
        if conversion.convert_argb_to_rgb() {
            let (rgb, mask) = Self::convert_argb_jpx_bitmap_to_rgb(
                &bitmap,
                dict.get_integer_for(b"SMaskInData", 0) == 1,
            )?;
            bitmap = rgb;
            *self.inline_mask.get_mut() = mask;
        } else if self
            .color_space
            .as_ref()
            .is_some_and(|space| space.family() == ColorSpaceFamily::Indexed)
            && self.bpc < 8
        {
            let shift = 8 - self.bpc;
            for row in 0..info.height as i32 {
                for sample in bitmap
                    .scanline_mut(row)?
                    .iter_mut()
                    .take(conversion.width() as usize)
                {
                    *sample = (u32::from(*sample) >> shift) as u8;
                }
            }
        }
        self.bpc = 8;
        self.cached_bitmap = Some(bitmap);
        Some(())
    }

    fn convert_argb_jpx_bitmap_to_rgb(
        bitmap: &CfxDibitmap,
        extract_mask: bool,
    ) -> Option<(CfxDibitmap, Option<CfxDibitmap>)> {
        let mut rgb = CfxDibitmap::create(bitmap.width(), bitmap.height(), FxdibFormat::Bgr, 0)?;
        let mut mask = if extract_mask {
            Some(CfxDibitmap::create(
                bitmap.width(),
                bitmap.height(),
                FxdibFormat::EightBppRgb,
                0,
            )?)
        } else {
            None
        };
        for row in 0..bitmap.height() {
            let source = bitmap.scanline(row)?;
            let target = rgb.scanline_mut(row)?;
            let mut coverage = mask.as_mut().and_then(|mask| mask.scanline_mut(row));
            for (column, (input, output)) in source
                .chunks_exact(4)
                .take(bitmap.width() as usize)
                .zip(target.chunks_exact_mut(3))
                .enumerate()
            {
                // SMaskInData 1 supplies unassociated color and opacity. The
                // detached mask applies that opacity during composition;
                // prematting RGB onto white here would apply it twice.
                output.copy_from_slice(&input[..3]);
                if let Some(coverage) = &mut coverage {
                    coverage[column] = input[3];
                }
            }
        }
        Some((rgb, mask))
    }

    fn disable_std_conversion(&self) {
        if self.std_cs {
            if let Some(space) = &self.color_space {
                space.enable_std_conversion(false);
            }
        }
    }

    fn trans_mask(&self) -> bool {
        self.load_mask
            && self.group_family == ColorSpaceFamily::DeviceCmyk
            && self.family == ColorSpaceFamily::DeviceCmyk
    }

    fn start_load_mask(
        &mut self,
        dict: &CpdfDictionary,
        context: super::cpdf_image::CpdfImageLoadContext<'_>,
    ) -> LoadState {
        self.matte_color = u32::MAX;
        if self.inline_mask.get_mut().is_some() {
            return LoadState::Success;
        }
        let soft = dict.get_stream_for(b"SMask");
        let Some(stream) = soft
            .as_ref()
            .map(Rc::clone)
            .or_else(|| dict.get_stream_for(b"Mask"))
        else {
            return LoadState::Success;
        };
        if soft.is_some() {
            let mask_dict = stream
                .borrow()
                .as_any()
                .downcast_ref::<CpdfStream>()
                .unwrap()
                .get_dict();
            if let Some(matte) = self.load_matte_color(&mask_dict) {
                self.matte_color = matte;
            }
        }
        let mask_space = {
            let mask_dict = stream
                .borrow()
                .as_any()
                .downcast_ref::<CpdfStream>()
                .unwrap()
                .get_dict();
            let borrowed = mask_dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_direct_object_for(b"ColorSpace")
                .and_then(|object| context.page_data.get_color_space(&object, None))
        };
        let mut mask_context =
            super::cpdf_image::CpdfImageLoadContext::new(context.page_data, None);
        mask_context.std_cs = true;
        // Mask streams have no resource scopes or size hints and do not load
        // another mask. An immediate mask failure is discarded, not propagated.
        let Some((mask, state)) =
            Self::start_load_with_context(stream, mask_space, mask_context, false)
        else {
            return LoadState::Success;
        };
        *self.external_mask.get_mut() = Some(Box::new(mask));
        if state == LoadState::Continue && self.jbig_state != LoadState::Continue {
            self.mask_pending = true;
        }
        state
    }

    fn continue_load_mask(
        &mut self,
        pause: Option<&mut dyn crate::fxcrt::pauseindicator_iface::PauseIndicatorIface>,
    ) -> LoadState {
        let Some(mask) = self.external_mask.get_mut().as_mut() else {
            return LoadState::Success;
        };
        let state = mask.continue_load(pause);
        if state == LoadState::Continue {
            return state;
        }
        self.disable_std_conversion();
        if state == LoadState::Fail {
            *self.external_mask.get_mut() = None;
        }
        state
    }

    fn load_matte_color(&self, mask_dict: &ObjectRef) -> Option<u32> {
        let space = self.color_space.as_ref()?;
        if self.family == ColorSpaceFamily::Pattern || space.component_count() > self.components {
            return None;
        }
        let borrowed = mask_dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        let matte = dict.get_array_for(b"Matte")?;
        let borrowed = matte.borrow();
        let matte = borrowed.as_any().downcast_ref::<CpdfArray>()?;
        if matte.size() != self.components as usize {
            return None;
        }
        let values: Vec<_> = (0..self.components as usize)
            .map(|component| matte.get_float_at(component))
            .collect();
        let rgb = space.get_rgb_or_zeros_on_error(&values);
        let convert = |value: f32| (value * 255.0).round() as i32 as u32;
        Some(crate::fxge::dib::fx_dib::argb_encode(
            0,
            convert(rgb.red),
            convert(rgb.green),
            convert(rgb.blue),
        ))
    }

    pub fn matte_color(&self) -> u32 {
        self.matte_color
    }

    pub fn detach_mask(&self) -> Option<super::cpdf_image::CpdfImageBitmap> {
        // Native inline JPX bytes become mask_ only in StartLoadMask().
        if self.has_mask {
            if let Some(mask) = self.inline_mask.borrow_mut().take() {
                return Some(super::cpdf_image::CpdfImageBitmap::Bitmap(mask));
            }
        }
        self.external_mask
            .borrow_mut()
            .take()
            .map(super::cpdf_image::CpdfImageBitmap::Dib)
    }

    fn get_decode_and_mask_array(&mut self, dict: &CpdfDictionary) {
        let space = self.color_space.as_ref().unwrap();
        let max_data = (1 << self.bpc) - 1;
        let decode = dict.get_array_for(b"Decode");
        let borrowed = decode.as_ref().map(|object| object.borrow());
        let decode = borrowed
            .as_ref()
            .and_then(|object| object.as_any().downcast_ref::<CpdfArray>());
        for i in 0..self.components as usize {
            let (_, min, mut max) = space.get_default_value(i);
            if self.family == ColorSpaceFamily::Indexed {
                max = max_data as f32;
            }
            let (decode_min, decode_max) = if let Some(decode) = decode {
                let lo = decode.get_float_at(i * 2);
                let hi = decode.get_float_at(i * 2 + 1);
                if lo != min || hi != max {
                    self.default_decode = false;
                }
                (lo, hi)
            } else {
                (min, max)
            };
            self.comp_data.push(DibCompData {
                decode_min,
                decode_step: (decode_max - decode_min) / max_data as f32,
                ..Default::default()
            });
        }
        if dict.key_exist(b"SMask") {
            return;
        }
        let Some(mask) = dict.get_array_for(b"Mask") else {
            return;
        };
        let borrowed = mask.borrow();
        let array = borrowed.as_any().downcast_ref::<CpdfArray>().unwrap();
        if array.size() >= self.components as usize * 2 {
            for (i, data) in self.comp_data.iter_mut().enumerate() {
                data.color_key_min = array.get_integer_at(i * 2).max(0) as u32;
                data.color_key_max = array.get_integer_at(i * 2 + 1).clamp(0, max_data) as u32;
            }
        }
        self.color_key = true;
    }

    fn load_palette(&mut self) {
        let Some(space) = &self.color_space else {
            return;
        };
        let bits = self.bpc * self.components;
        if self.family == ColorSpaceFamily::Pattern || bits > 8 {
            return;
        }
        if bits == 1 {
            if self.default_decode
                && matches!(
                    self.family,
                    ColorSpaceFamily::DeviceGray | ColorSpaceFamily::DeviceRgb
                )
            {
                return;
            }
            if space.component_count() > 3 {
                return;
            }
            let mut values = [self.comp_data[0].decode_min; 3];
            let reset = rgb_argb(space.get_rgb_or_zeros_on_error(&values));
            let set = if space
                .as_indexed_cs()
                .is_some_and(|indexed| indexed.max_index() == 0)
            {
                0xff00_0000
            } else {
                for value in &mut values {
                    *value += self.comp_data[0].decode_step;
                }
                rgb_argb(space.get_rgb_or_zeros_on_error(&values))
            };
            if reset != 0xff00_0000 || set != 0xffff_ffff {
                self.palette = vec![reset, set];
            }
            return;
        }
        // JPX can adopt DeviceGray after LoadColorInfo left family Unknown.
        // Match native LoadPalette's check of the actual stock color space.
        if self.bpc == 8
            && self.default_decode
            && Rc::ptr_eq(space, &get_stock_device_cs(ColorSpaceFamily::DeviceGray))
        {
            return;
        }
        self.palette = vec![0; 256];
        let mut values = vec![0.0; self.components.max(16) as usize];
        for index in 0..1usize << bits {
            let mut color_data = index;
            for (value, data) in values.iter_mut().zip(&self.comp_data) {
                let encoded = color_data % (1 << self.bpc);
                color_data /= 1 << self.bpc;
                *value = data.decode_min + data.decode_step * encoded as f32;
            }
            let rgb = if self.components == 1
                && self.family == ColorSpaceFamily::IccBased
                && space.component_count() > 1
            {
                space.get_rgb_or_zeros_on_error(&vec![values[0]; space.component_count() as usize])
            } else {
                space.get_rgb_or_zeros_on_error(&values)
            };
            self.palette[index] = rgb_argb(rgb);
        }
    }

    // Batch only normal eight-bit ICC Decode rows. The profile owns reusable
    // lookup/scratch storage; ordinary image buffers and paths stay unchanged.
    fn try_translate_icc_decode_scanline(&self, dest: &mut [u8], src: &[u8]) -> bool {
        if self.default_decode || self.bpc != 8 || self.family != ColorSpaceFamily::IccBased {
            return false;
        }
        let Some(space) = self.color_space.as_ref() else {
            return false;
        };
        let Some(profile) = space.get_icc_profile() else {
            return false;
        };
        let comps = self.components as usize;
        if self.components != space.component_count()
            || !matches!(comps, 1 | 3 | 4)
            || self.comp_data.len() != comps
        {
            return false;
        }
        let Ok(pixels) = usize::try_from(self.width) else {
            return false;
        };
        let mut decode = [[0.0f32; 2]; 4];
        for (entry, data) in decode.iter_mut().zip(&self.comp_data) {
            *entry = [data.decode_min, data.decode_step];
        }
        profile.translate_decoded_scanline(dest, src, pixels, &decode[..comps])
    }

    fn translate_scanline24bpp(&self, dest: &mut [u8], src: &[u8]) {
        #[cfg(feature = "render-profile")]
        let _profile = crate::render_profile::Scope::new("DIB RGB conversion");
        let space = self.color_space.as_ref().unwrap();
        if self.default_decode {
            if matches!(
                self.family,
                ColorSpaceFamily::DeviceRgb | ColorSpaceFamily::CalRgb
            ) {
                if self.components != 3 {
                    return;
                }
                match self.bpc {
                    8 => {
                        for (input, output) in src
                            .chunks_exact(3)
                            .take(self.width as usize)
                            .zip(dest.chunks_exact_mut(3))
                        {
                            output.copy_from_slice(&[input[2], input[1], input[0]]);
                        }
                    }
                    16 => {
                        for (input, output) in src
                            .chunks_exact(6)
                            .take(self.width as usize)
                            .zip(dest.chunks_exact_mut(3))
                        {
                            output.copy_from_slice(&[input[4], input[2], input[0]]);
                        }
                    }
                    _ => {
                        let max_data = (1 << self.bpc) - 1;
                        let mut bit = 0;
                        for output in dest.chunks_exact_mut(3).take(self.width as usize) {
                            let rgb: [u8; 3] = std::array::from_fn(|_| {
                                let value = get_bits(src, bit, self.bpc);
                                bit += self.bpc as usize;
                                (value * 255 / max_data) as u8
                            });
                            output.copy_from_slice(&[rgb[2], rgb[1], rgb[0]]);
                        }
                    }
                }
                return;
            }
            if self.bpc == 8 {
                if self.components == space.component_count() {
                    space.translate_image_line(
                        dest,
                        src,
                        self.width as usize,
                        self.width,
                        self.height,
                        self.trans_mask(),
                    );
                }
                return;
            }
        }
        if self.bpc == 16 && self.components == 1 && self.family == ColorSpaceFamily::DeviceGray {
            // DeviceGray's GetRGB clamps one decoded sample and repeats it.
            // Keep the full 16-bit sample and float decode (not just its high
            // byte), without a values allocation or virtual calls per pixel.
            let data = &self.comp_data[0];
            for (input, output) in src
                .chunks_exact(2)
                .take(self.width as usize)
                .zip(dest.chunks_exact_mut(3))
            {
                let sample = u16::from_be_bytes([input[0], input[1]]);
                let gray = ((data.decode_min + data.decode_step * f32::from(sample))
                    .clamp(0.0, 1.0)
                    * 255.0) as u8;
                output.copy_from_slice(&[gray; 3]);
            }
            return;
        }
        if self.bpc == 8 && self.components == 3 && self.family == ColorSpaceFamily::DeviceRgb {
            // DeviceRGB's GetRGB only clamps each decoded component. Keep
            // the same float operations and byte truncation, without building
            // a values vector or making virtual color-space calls per pixel.
            let decode = |value: u8, data: &DibCompData| {
                ((data.decode_min + data.decode_step * f32::from(value)).clamp(0.0, 1.0) * 255.0)
                    as u8
            };
            for (input, output) in src
                .chunks_exact(3)
                .take(self.width as usize)
                .zip(dest.chunks_exact_mut(3))
            {
                output.copy_from_slice(&[
                    decode(input[2], &self.comp_data[2]),
                    decode(input[1], &self.comp_data[1]),
                    decode(input[0], &self.comp_data[0]),
                ]);
            }
            return;
        }
        if self.try_translate_icc_decode_scanline(dest, src) {
            return;
        }
        self.translate_decoded_scanline_scalar(dest, src);
    }

    fn translate_decoded_scanline_scalar(&self, dest: &mut [u8], src: &[u8]) {
        let space = self.color_space.as_ref().unwrap();
        let mut values = vec![0.0; self.components.max(16) as usize];
        let mut bit = 0;
        for pixel in dest.chunks_exact_mut(3).take(self.width as usize) {
            for (value, data) in values.iter_mut().zip(&self.comp_data) {
                *value = data.decode_min + data.decode_step * get_bits(src, bit, self.bpc) as f32;
                bit += self.bpc as usize;
            }
            let rgb = if self.trans_mask() {
                let key = 1.0 - values[3];
                FxRgbStruct {
                    red: (1.0 - values[0]) * key,
                    green: (1.0 - values[1]) * key,
                    blue: (1.0 - values[2]) * key,
                }
            } else if self.family == ColorSpaceFamily::Pattern {
                FxRgbStruct::default()
            } else {
                space.get_rgb_or_zeros_on_error(&values)
            };
            pixel.copy_from_slice(&[
                (rgb.blue.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.green.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.red.clamp(0.0, 1.0) * 255.0) as u8,
            ]);
        }
    }

    pub fn get_scanline(&self, row: i32) -> DibScanline<'_> {
        #[cfg(feature = "render-profile")]
        let _profile = crate::render_profile::Scope::new("DIB scanline other");
        let cached = self
            .cached_bitmap
            .as_ref()
            .filter(|bitmap| self.source_pitch <= bitmap.pitch() as usize);
        if self.color_space.is_none() && !self.color_key && self.bpc * self.components > 8 {
            if let Some(bitmap) = cached {
                return DibScanline(DibScanlineStorage::Cached(
                    bitmap
                        .scanline(row.min(bitmap.height() - 1))
                        .unwrap_or_default(),
                ));
            }
        }
        let mut buffers = self.buffers.borrow_mut();
        if let Some((previous, len)) = buffers
            .history
            .as_ref()
            .and_then(|history| history.lookup(row))
        {
            return DibScanline(DibScanlineStorage::Buffer(RefMut::map(
                buffers,
                |buffers| {
                    if let Some(index) = previous {
                        &mut buffers.history.as_mut().unwrap().previous[index].pixels[..len]
                    } else if self.color_key {
                        &mut buffers.mask[..len]
                    } else {
                        &mut buffers.line[..len]
                    }
                },
            )));
        }
        let retained = self.stream_acc.get_span();
        let mut decoder = self.scanline_decoder.borrow_mut();
        if self.scanline_history_bytes != 0
            && decoder.as_ref().is_some_and(|decoder| {
                let next = decoder.state().next_line;
                next < 0 || (next > row && next != row.wrapping_add(1))
            })
        {
            if let Some(history) = &mut buffers.history {
                // Rows retained before a real rewind must not leak across a
                // decoder restart, including malformed predictor recovery.
                history.clear();
            }
        }
        buffers.prepare_scanline(self.color_key);
        let offset = usize::try_from(row)
            .ok()
            .and_then(|row| row.checked_mul(self.source_pitch));
        let remaining = if let Some(bitmap) = cached {
            bitmap
                .scanline(row.min(bitmap.height() - 1))
                .unwrap_or_default()
        } else if let Some(decoder) = decoder.as_mut() {
            decoder.get_scanline(row)
        } else {
            offset
                .and_then(|offset| retained.get(offset..))
                .unwrap_or_default()
        };
        if remaining.is_empty() {
            let mask = self.color_key;
            let result = if mask {
                &mut buffers.mask
            } else {
                &mut buffers.line
            };
            result.fill(0);
            return DibScanline(DibScanlineStorage::Buffer(RefMut::map(
                buffers,
                |buffers| {
                    if mask {
                        &mut buffers.mask[..]
                    } else {
                        &mut buffers.line[..]
                    }
                },
            )));
        }
        let ScanBuffers {
            line,
            mask,
            remainder,
            ..
        } = &mut *buffers;
        let src = if remaining.len() < self.source_pitch {
            remainder.resize(self.source_pitch, 0);
            remainder.fill(0);
            remainder[..remaining.len()].copy_from_slice(remaining);
            remainder.as_slice()
        } else {
            &remaining[..self.source_pitch]
        };
        let width = self.width as usize;
        let result_len;
        if self.bpc * self.components == 1 {
            if self.image_mask && self.default_decode {
                for (dest, src) in line.iter_mut().zip(src) {
                    *dest = !src;
                }
                result_len = self.source_pitch;
            } else if !self.color_key {
                line[..src.len()].copy_from_slice(src);
                result_len = src.len();
            } else {
                let data = &self.comp_data[0];
                let reset = if data.color_key_min == 0 {
                    0
                } else {
                    self.palette.first().copied().unwrap_or(0xff00_0000)
                };
                let set = if data.color_key_max == 1 {
                    0
                } else {
                    self.palette.get(1).copied().unwrap_or(0xffff_ffff)
                };
                for (col, dest) in mask.chunks_exact_mut(4).take(width).enumerate() {
                    dest.copy_from_slice(
                        &if src[col / 8] & (0x80 >> (col % 8)) != 0 {
                            set
                        } else {
                            reset
                        }
                        .to_le_bytes(),
                    );
                }
                result_len = width * 4;
            }
        } else if self.bpc * self.components <= 8 {
            if self.bpc == 8 {
                line[..src.len()].copy_from_slice(src);
            } else {
                let mut bit = 0;
                for dest in line.iter_mut().take(width) {
                    let mut index = 0;
                    for color in 0..self.components {
                        index |= get_bits(src, bit, self.bpc) << (color * self.bpc);
                        bit += self.bpc as usize;
                    }
                    *dest = index as u8;
                }
            }
            if self.color_key {
                let data = &self.comp_data[0];
                for (&index, dest) in line.iter().take(width).zip(mask.chunks_exact_mut(4)) {
                    let color = if self.palette.is_empty() {
                        u32::from(index) * 0x010101
                    } else {
                        self.palette[index as usize]
                    };
                    dest.copy_from_slice(&color.to_le_bytes());
                    dest[3] = if u32::from(index) < data.color_key_min
                        || u32::from(index) > data.color_key_max
                    {
                        255
                    } else {
                        0
                    };
                }
                result_len = width * 4;
            } else {
                result_len = width;
            }
        } else {
            if self.color_key {
                let mut bit = 0;
                for dest in mask.chunks_exact_mut(4).take(width) {
                    let mut opaque = false;
                    for data in &self.comp_data {
                        let value = get_bits(src, bit, self.bpc);
                        bit += self.bpc as usize;
                        opaque |= value < data.color_key_min || value > data.color_key_max;
                    }
                    dest[3] = if opaque { 255 } else { 0 };
                }
            }
            let colors = if self.color_space.is_some() {
                self.translate_scanline24bpp(line, src);
                &line[..width * 3]
            } else {
                &src[..width * 3]
            };
            if self.color_key {
                for (src, dest) in colors
                    .chunks_exact(3)
                    .take(width)
                    .zip(mask.chunks_exact_mut(4))
                {
                    dest[..3].copy_from_slice(src);
                }
                result_len = width * 4;
            } else {
                result_len = width * 3;
            }
        }
        if row >= 0 && row < self.height {
            if let Some(history) = &mut buffers.history {
                history.current = Some((row, result_len));
            }
        }
        DibScanline(DibScanlineStorage::Buffer(RefMut::map(
            buffers,
            |buffers| {
                if self.color_key {
                    &mut buffers.mask[..result_len]
                } else {
                    &mut buffers.line[..result_len]
                }
            },
        )))
    }
}

impl StretchSource for CpdfDib {
    type Row<'a>
        = DibScanline<'a>
    where
        Self: 'a;
    type Palette<'a>
        = &'a [u32]
    where
        Self: 'a;
    fn width(&self) -> i32 {
        self.width
    }
    fn height(&self) -> i32 {
        self.height
    }
    fn format(&self) -> FxdibFormat {
        self.format
    }
    fn pitch(&self) -> u32 {
        self.pitch
    }
    fn palette(&self) -> &[u32] {
        &self.palette
    }
    fn scanline(&self, row: i32) -> DibScanline<'_> {
        self.get_scanline(row)
    }
    fn skip_to_scanline(
        &self,
        row: i32,
        mut pause: Option<&mut dyn crate::fxcrt::pauseindicator_iface::PauseIndicatorIface>,
    ) -> bool {
        if self.scanline_history_bytes != 0
            && self
                .buffers
                .borrow()
                .history
                .as_ref()
                .is_some_and(|history| history.lookup(row).is_some())
        {
            return false;
        }
        // ScanlineDecoder::SkipToScanline with short decoder borrows: native
        // pause callbacks can re-enter GetScanline after each completed row.
        {
            let mut slot = self.scanline_decoder.borrow_mut();
            let Some(decoder) = slot.as_mut() else {
                return false;
            };
            let next = decoder.state().next_line;
            if next == row || next == row.wrapping_add(1) {
                return false;
            }
            if next < 0 || next > row {
                if !decoder.rewind() {
                    return false;
                }
                decoder.state_mut().next_line = 0;
                if let Some(history) = &mut self.buffers.borrow_mut().history {
                    history.clear();
                }
            }
            decoder.state_mut().has_last_scanline = false;
        }
        loop {
            {
                let mut slot = self.scanline_decoder.borrow_mut();
                let Some(decoder) = slot.as_mut() else {
                    return false;
                };
                if decoder.state().next_line >= row {
                    return false;
                }
                let has_line = decoder.get_next_line();
                decoder.state_mut().has_last_scanline = has_line;
                decoder.state_mut().next_line += 1;
            }
            if pause
                .as_mut()
                .is_some_and(|pause| pause.need_to_pause_now())
            {
                return true;
            }
        }
    }
    fn estimated_image_memory_burden(&self) -> usize {
        self.cached_bitmap
            .as_ref()
            .map_or(0, CfxDibitmap::estimated_image_memory_burden)
            + self.scanline_history_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_colorspace::get_stock_cs_for_name;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn dict(width: i32, height: i32, bpc: i32, space: &[u8]) -> CpdfDictionary {
        let mut dict = CpdfDictionary::new();
        for (key, value) in [
            (b"Width".as_slice(), width),
            (b"Height", height),
            (b"BitsPerComponent", bpc),
        ] {
            dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        if !space.is_empty() {
            dict.set_new_name_for(b"ColorSpace", space);
        }
        dict
    }

    fn array(values: &[i32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_signed(value));
        }
        array
    }

    #[test]
    fn packed_component_indices_use_low_component_bits_and_palette_decode() {
        let stream = CpdfStream::from_data_and_dict(
            vec![0x6c, 0xd0],
            retain_object(dict(2, 1, 2, b"DeviceRGB")),
        );
        let dib =
            CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceRGB")).unwrap();
        assert_eq!(dib.format(), FxdibFormat::EightBppRgb);
        assert_eq!(&*dib.get_scanline(0), &[57, 28]);
        assert_eq!(dib.palette()[57], 0xff55aaff);
        assert_eq!(dib.palette()[28], 0xff00ff55);
        // Palette decode is used for samples with <= 8 total bits, including
        // RGB. It must not be mistaken for a PDF Indexed color-space array.
        let bitmap = crate::fxge::dib::cfx_dibbase::realize(&dib).unwrap();
        assert_eq!(bitmap.palette()[57], 0xff55aaff);
    }

    #[test]
    fn sixteen_bit_decode_and_color_keys_keep_original_sample_precision() {
        let mut dictionary = dict(2, 2, 16, b"DeviceGray");
        dictionary.set_new_for(b"Decode", array(&[1, 0]));
        dictionary.set_new_for(b"Mask", array(&[1, 1]));
        let stream = CpdfStream::from_data_and_dict(vec![0, 1, 0, 2], retain_object(dictionary));
        let dib =
            CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceGray")).unwrap();
        assert_eq!(dib.format(), FxdibFormat::Bgra);
        assert_eq!(
            &*dib.get_scanline(0),
            &[254, 254, 254, 0, 254, 254, 254, 255]
        );
        // A fully missing scanline is zeroed, including its alpha channel.
        assert_eq!(&*dib.get_scanline(1), &[0; 8]);

        let stream = CpdfStream::from_data_and_dict(
            vec![0x12, 0xff, 0x34, 0xee, 0x56, 0xdd],
            retain_object(dict(1, 1, 16, b"DeviceRGB")),
        );
        let dib =
            CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceRGB")).unwrap();
        assert_eq!(&*dib.get_scanline(0), &[0x56, 0x34, 0x12]);
    }

    #[test]
    fn truncated_rows_pad_samples_and_mask_decode_distinguishes_missing_rows() {
        let stream = CpdfStream::from_data_and_dict(
            vec![255, 0, 0, 0, 255],
            retain_object(dict(2, 2, 8, b"DeviceRGB")),
        );
        let dib =
            CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceRGB")).unwrap();
        assert_eq!(&*dib.get_scanline(0), &[0, 0, 255, 0, 255, 0]);
        assert_eq!(&*dib.get_scanline(1), &[0; 8]);
        let stream = CpdfStream::from_data_and_dict(vec![0x80], retain_object(dict(16, 2, 0, b"")));
        let dib = CpdfDib::load(retain_object(stream), None).unwrap();
        assert_eq!(dib.format(), FxdibFormat::OneBppMask);
        assert_eq!(&*dib.get_scanline(0), &[0x7f, 0xff]);
        assert_eq!(&*dib.get_scanline(1), &[0; 4]);
    }
    #[test]
    fn image_decoder_pipeline_retains_bytes_from_preceding_filters() {
        let compressed = crate::fxcodec::flate::encode(&[1, 2, 3]);
        let encoded: Vec<u8> = compressed
            .iter()
            .flat_map(|byte| format!("{byte:02X}").into_bytes())
            .chain(std::iter::once(b'>'))
            .collect();
        let mut dictionary = dict(1, 1, 8, b"DeviceRGB");
        let mut filters = CpdfArray::new();
        filters.append_new_name(b"ASCIIHexDecode");
        filters.append_new_name(b"FlateDecode");
        dictionary.set_new_for(b"Filter", filters);
        let stream = CpdfStream::from_data_and_dict(encoded, retain_object(dictionary));
        let dib =
            CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceRGB")).unwrap();
        assert_eq!(&*dib.get_scanline(0), &[3, 2, 1]);
    }
    #[test]
    fn flate_images_decode_partial_rows_and_retain_predictor_rewind_state() {
        let mut dictionary = dict(1, 2, 8, b"DeviceGray");
        dictionary.set_new_name_for(b"Filter", b"FlateDecode");
        let mut params = CpdfDictionary::new();
        params.set_new_for(b"Predictor", CpdfScalarObject::number_from_signed(12));
        dictionary.set_new_for(b"DecodeParms", params);
        let compressed = crate::fxcodec::flate::encode(&[2, 64, 2]);
        let stream = CpdfStream::from_data_and_dict(compressed, retain_object(dictionary));
        let dib =
            CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceGray")).unwrap();
        assert_eq!(&*dib.get_scanline(0), &[64]);
        // The partial Up row is zero-filled before prediction.
        assert_eq!(&*dib.get_scanline(1), &[64]);
        // Native Rewind resets the inflater/leftover count, retaining last_line.
        assert_eq!(&*dib.get_scanline(0), &[128]);

        let mut dictionary = dict(8, 2, 0, b"");
        dictionary.set_new_name_for(b"Filter", b"FlateDecode");
        let stream = CpdfStream::from_data_and_dict(
            crate::fxcodec::flate::encode(&[0x80]),
            retain_object(dictionary),
        );
        let dib = CpdfDib::load(retain_object(stream), None).unwrap();
        assert_eq!(&*dib.get_scanline(0), &[0x7f]);
        // An exhausted Flate decoder supplies zero samples, which are inverted
        // by the stencil Decode; it does not supply a missing scanline.
        assert_eq!(&*dib.get_scanline(1), &[0xff]);
    }

    #[test]
    fn retained_flate_rows_avoid_overlap_rewinds_and_account_memory() {
        use crate::fxcodec::scanlinedecoder::ScanlineState;
        use std::cell::Cell;
        struct CountingDecoder {
            inner: Box<dyn ScanlineDecoder>,
            rewinds: Rc<Cell<usize>>,
        }
        impl ScanlineDecoder for CountingDecoder {
            fn state(&self) -> &ScanlineState {
                self.inner.state()
            }
            fn state_mut(&mut self) -> &mut ScanlineState {
                self.inner.state_mut()
            }
            fn rewind(&mut self) -> bool {
                self.rewinds.set(self.rewinds.get() + 1);
                self.inner.rewind()
            }
            fn get_next_line(&mut self) -> bool {
                self.inner.get_next_line()
            }
            fn current_line(&self) -> &[u8] {
                self.inner.current_line()
            }
            fn get_src_offset(&self) -> u32 {
                self.inner.get_src_offset()
            }
        }
        let mut dictionary = dict(32, 16, 8, b"DeviceRGB");
        dictionary.set_new_name_for(b"Filter", b"FlateDecode");
        let samples: Vec<u8> = (0..16)
            .flat_map(|row| (0..32).flat_map(move |x| [row, x, row + x]))
            .collect();
        let stream = CpdfStream::from_data_and_dict(
            crate::fxcodec::flate::encode(&samples),
            retain_object(dictionary),
        );
        let mut dib =
            CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceRGB")).unwrap();
        assert_eq!(dib.estimated_image_memory_burden(), 0);
        dib.retain_scanline_history();
        let bytes = dib.pitch() as usize * 2;
        assert_eq!(dib.estimated_image_memory_burden(), bytes);
        let rewinds = Rc::new(Cell::new(0));
        let inner = dib.scanline_decoder.get_mut().take().unwrap();
        *dib.scanline_decoder.get_mut() = Some(Box::new(CountingDecoder {
            inner,
            rewinds: Rc::clone(&rewinds),
        }));
        let check = |row: i32| {
            let pixels = dib.get_scanline(row);
            let expected: Vec<u8> = (0..32u8)
                .flat_map(|x| [row as u8 + x, x, row as u8])
                .collect();
            assert_eq!(&*pixels, expected.as_slice());
            // Accounting must also work while the borrowed row is live.
            assert_eq!(dib.estimated_image_memory_burden(), bytes);
        };
        for row in 0..8 {
            check(row);
        }
        assert_eq!(rewinds.get(), 1);
        for row in [5, 6, 7, 6, 7, 8] {
            assert!(!dib.skip_to_scanline(row, None));
            check(row);
        }
        assert_eq!(rewinds.get(), 1, "three-row overlap must not restart Flate");
        check(0);
        assert_eq!(rewinds.get(), 2);
        assert!(dib
            .buffers
            .borrow()
            .history
            .as_ref()
            .unwrap()
            .lookup(8)
            .is_none());
        for row in 1..16 {
            check(row);
        }
        assert_eq!(rewinds.get(), 2);
        assert!(ScanlineHistory::new(SCANLINE_HISTORY_BYTES / 2 + 1).is_none());
        assert!(ScanlineHistory::new(usize::MAX).is_none());
    }

    #[test]
    fn run_length_images_validate_size_and_keep_missing_rows_distinct() {
        let make = |data: Vec<u8>| {
            let mut dictionary = dict(8, 3, 0, b"");
            dictionary.set_new_name_for(b"Filter", b"RunLengthDecode");
            CpdfDib::load(
                retain_object(CpdfStream::from_data_and_dict(
                    data,
                    retain_object(dictionary),
                )),
                None,
            )
        };
        // One advertised byte cannot cover three rows.
        assert!(make(vec![0, 0x80, 128]).is_none());
        // Three advertised literal bytes pass the native size precheck, even
        // when the encoded literal is truncated after its first byte.
        let dib = make(vec![2, 0x80]).unwrap();
        assert_eq!(&*dib.get_scanline(0), &[0x7f]);
        assert_eq!(&*dib.get_scanline(1), &[0; 4]);
        assert_eq!(&*dib.get_scanline(2), &[0; 4]);
        assert_eq!(&*dib.get_scanline(0), &[0x7f]);
    }

    #[test]
    fn retained_image_skip_releases_decoder_before_pause_callback() {
        let mut dictionary = dict(1, 2, 8, b"DeviceGray");
        dictionary.set_new_name_for(b"Filter", b"FlateDecode");
        let stream = CpdfStream::from_data_and_dict(
            crate::fxcodec::flate::encode(&[64, 128]),
            retain_object(dictionary),
        );
        let mut dib =
            CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceGray")).unwrap();
        dib.retain_scanline_history();
        struct Reenter<'a>(&'a CpdfDib);
        impl crate::fxcrt::pauseindicator_iface::PauseIndicatorIface for Reenter<'_> {
            fn need_to_pause_now(&mut self) -> bool {
                assert_eq!(&*self.0.get_scanline(0), &[64]);
                true
            }
        }
        assert!(dib.skip_to_scanline(1, Some(&mut Reenter(&dib))));
        assert!(!dib.skip_to_scanline(1, None));
        assert_eq!(&*dib.get_scanline(1), &[128]);
    }

    #[test]
    fn fax_image_creation_checks_provided_pitch_before_scanlines() {
        let mut params = CpdfDictionary::new();
        params.set_new_for(b"K", CpdfScalarObject::number_from_signed(-1));
        params.set_new_for(b"Columns", CpdfScalarObject::number_from_signed(8));
        let mut dictionary = dict(16, 1, 1, b"");
        dictionary.set_new_name_for(b"Filter", b"CCITTFaxDecode");
        dictionary.set_new_for(b"DecodeParms", params);
        let stream = CpdfStream::from_data_and_dict(vec![0xff], retain_object(dictionary));
        assert!(CpdfDib::load(retain_object(stream), None).is_none());
    }

    #[test]
    fn fax_uses_native_bpc_override_decoder_pitch_and_stencil_polarity() {
        let cases: [(i32, &[u8]); 2] = [
            (
                -1,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/fax_g4.bin"
                )),
            ),
            (
                0,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/fax_g3.bin"
                )),
            ),
        ];
        for (k, bytes) in cases {
            for black_is_one in [false, true] {
                let mut params = CpdfDictionary::new();
                params.set_new_for(b"K", CpdfScalarObject::number_from_signed(k));
                params.set_new_for(b"Columns", CpdfScalarObject::number_from_signed(16));
                params.set_new_for(b"Rows", CpdfScalarObject::number_from_signed(1));
                params.set_new_for(b"BlackIs1", CpdfScalarObject::boolean(black_is_one));
                let mut dictionary = dict(16, 2, 16, b"");
                dictionary.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
                dictionary.set_new_name_for(b"Filter", b"CCITTFaxDecode");
                dictionary.set_new_for(b"DecodeParms", params);
                let stream = retain_object(CpdfStream::from_data_and_dict(
                    bytes.to_vec(),
                    retain_object(dictionary),
                ));
                let dib = CpdfDib::load(stream, None).unwrap();
                assert_eq!(dib.format(), FxdibFormat::OneBppMask);
                let first = dib.get_scanline(0);
                assert_eq!(first[0] & 0x80 != 0, black_is_one);
                assert_eq!(first[1] & 1 != 0, !black_is_one);
                drop(first);
                let second = dib.get_scanline(1);
                assert_eq!(second[0] & 0x80 != 0, !black_is_one);
                assert_eq!(second[1] & 1 != 0, black_is_one);
            }
        }
    }

    #[test]
    fn jbig2_continuation_retains_filtered_globals_and_decodes_into_existing_bits() {
        use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
        use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
        let objects = CpdfIndirectObjectHolder::new();
        let mut globals_dict = CpdfDictionary::new();
        globals_dict.set_new_name_for(b"Filter", b"FlateDecode");
        let globals = retain_object(CpdfStream::from_data_and_dict(
            crate::fxcodec::flate::encode(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/jbig2_globals.bin"
            ))),
            retain_object(globals_dict),
        ));
        objects.borrow_mut().add_indirect_object(globals.clone());
        let mut params = CpdfDictionary::new();
        params.set_new_for(
            b"JBIG2Globals",
            CpdfReference::new(&objects, globals.borrow().obj_num()),
        );
        let mut dictionary = dict(1152, 720, 8, b"DeviceGray");
        dictionary.set_new_name_for(b"Filter", b"JBIG2Decode");
        dictionary.set_new_for(b"DecodeParms", params);
        dictionary.set_new_for(b"Mask", array(&[0, 0]));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/jbig2_page.bin"
            ))
            .to_vec(),
            retain_object(dictionary),
        ));
        let data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
        let (mut dib, state) = CpdfDib::start_load_with_context(
            stream,
            get_stock_cs_for_name(b"DeviceGray"),
            super::super::cpdf_image::CpdfImageLoadContext::new(&data, None),
            false,
        )
        .unwrap();
        assert_eq!(state, LoadState::Continue);
        let pointer = dib.cached_bitmap.as_ref().unwrap().buffer().as_ptr();
        struct Pause;
        impl crate::fxcrt::pauseindicator_iface::PauseIndicatorIface for Pause {
            fn need_to_pause_now(&mut self) -> bool {
                true
            }
        }
        assert_eq!(dib.continue_load(Some(&mut Pause)), LoadState::Continue);
        assert!(dib
            .global_acc
            .as_ref()
            .is_some_and(|acc| !acc.get_span().is_empty()));
        assert!(dib
            .cached_bitmap
            .as_ref()
            .unwrap()
            .buffer()
            .iter()
            .all(|&byte| byte == 0));
        assert_eq!(dib.continue_load(None), LoadState::Success);
        assert_eq!(
            data.get_or_create_codec_context()
                .borrow()
                .symbol_dictionary_keys()
                .collect::<Vec<_>>(),
            [(1u64 << 32, 11)]
        );
        assert_eq!(
            dib.cached_bitmap.as_ref().unwrap().buffer().as_ptr(),
            pointer
        );
        assert_eq!(dib.format(), FxdibFormat::Bgra);
        assert!(dib.estimated_image_memory_burden() < 1152 * 720 / 4);
        let mut saw_opaque = false;
        let mut saw_transparent = false;
        for row in 0..720 {
            for pixel in dib.get_scanline(row).chunks_exact(4) {
                assert!(pixel == [255, 255, 255, 255] || pixel == [0, 0, 0, 0]);
                saw_opaque |= pixel[3] == 255;
                saw_transparent |= pixel[3] == 0;
            }
        }
        assert!(saw_opaque && saw_transparent);
    }
    #[test]
    fn image_depth_overrides_use_native_filter_names_before_canonicalization() {
        let jpeg = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/gray.jpg"
        ));
        for (name, expected) in [
            (b"DCTDecode".as_slice(), true),
            (b"DCTDecode\0tail", true),
            (b"DCT", false),
            (b"DCT\0tail", false),
        ] {
            let mut dictionary = dict(16, 1, 0, b"DeviceGray");
            dictionary.set_new_name_for(b"Filter", name);
            let stream = CpdfStream::from_data_and_dict(jpeg.to_vec(), retain_object(dictionary));
            assert_eq!(
                CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceGray"))
                    .is_some(),
                expected
            );
        }
        for (name, expected) in [
            (b"CCITTFaxDecode".as_slice(), true),
            (b"CCITTFaxDecode\0tail", true),
            (b"CCF", false),
            (b"CCF\0tail", false),
        ] {
            let mut params = CpdfDictionary::new();
            params.set_new_for(b"Columns", CpdfScalarObject::number_from_signed(16));
            let mut dictionary = dict(16, 1, 8, b"DeviceGray");
            dictionary.set_new_name_for(b"Filter", name);
            dictionary.set_new_for(b"DecodeParms", params);
            let stream = CpdfStream::from_data_and_dict(vec![255], retain_object(dictionary));
            assert_eq!(
                CpdfDib::load(retain_object(stream), get_stock_cs_for_name(b"DeviceGray"))
                    .is_some(),
                expected
            );
        }
    }

    #[test]
    fn jpeg_adopts_header_dimensions_and_overrides_pdf_component_depth() {
        let mut dictionary = dict(32, 4, 0, b"DeviceGray");
        dictionary.set_new_name_for(b"Filter", b"DCTDecode");
        let stream = retain_object(CpdfStream::from_data_and_dict(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/gray.jpg"
            ))
            .to_vec(),
            retain_object(dictionary),
        ));
        let dib = CpdfDib::load(stream, get_stock_cs_for_name(b"DeviceGray")).unwrap();
        assert_eq!(
            (dib.width(), dib.height(), dib.format()),
            (16, 1, FxdibFormat::EightBppRgb)
        );
        struct Pause;
        impl crate::fxcrt::pauseindicator_iface::PauseIndicatorIface for Pause {
            fn need_to_pause_now(&mut self) -> bool {
                true
            }
        }
        assert!(dib.skip_to_scanline(1, Some(&mut Pause)));
        // The skipped JPEG row is retained as ScanlineDecoder::last_scanline.
        let row = dib.get_scanline(0);
        assert!(row[0] > 240 && row[15] < 15);
    }

    #[test]
    fn cmyk_jpeg_decode_arrays_and_color_keys_use_decoded_cmyk_samples() {
        for key in [false, true] {
            let mut dictionary = dict(16, 1, 8, b"DeviceCMYK");
            dictionary.set_new_name_for(b"Filter", b"DCTDecode");
            dictionary.set_new_for(b"Decode", array(&[1, 0, 1, 0, 1, 0, 1, 0]));
            if key {
                dictionary.set_new_for(b"Mask", array(&[0, 255, 0, 255, 0, 255, 0, 255]));
            }
            let stream = retain_object(CpdfStream::from_data_and_dict(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/cmyk.jpg"
                ))
                .to_vec(),
                retain_object(dictionary),
            ));
            let dib = CpdfDib::load(stream, get_stock_cs_for_name(b"DeviceCMYK")).unwrap();
            let row = dib.get_scanline(0);
            // DeviceCMYK uses PDFium's Adobe table, rather than the codec's
            // direct subtractive RGB conversion. Verify decoded color polarity.
            assert!(row[2] > 200 && row[0] < 80);
            let stride = if key { 4 } else { 3 };
            let last = &row[15 * stride..];
            assert!(i32::from(last[0]) > i32::from(last[2]) + 75);
            if key {
                assert!(row.chunks_exact(4).all(|pixel| pixel[3] == 0));
            }
        }
    }
    #[test]
    fn jpx_grayscale_without_pdf_colorspace_preserves_samples_and_explicit_decode() {
        // Lossless 16x16 gray ramp: each row contains 0, 17, ..., 255.
        // The JP2 header supplies DeviceGray when the PDF omits ColorSpace/BPC.
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/gray.jp2"
        ));
        for (explicit_space, inverted) in [(false, false), (true, false), (true, true)] {
            let mut dictionary = dict(
                16,
                16,
                if explicit_space { 8 } else { 0 },
                if explicit_space { b"DeviceGray" } else { b"" },
            );
            dictionary.set_new_name_for(b"Filter", b"JPXDecode");
            if !explicit_space {
                dictionary.remove_for(b"BitsPerComponent");
            }
            if inverted {
                dictionary.set_new_for(b"Decode", array(&[1, 0]));
            }
            let stream = retain_object(CpdfStream::from_data_and_dict(
                bytes.to_vec(),
                retain_object(dictionary),
            ));
            let dib = CpdfDib::load(
                stream,
                explicit_space.then(|| get_stock_cs_for_name(b"DeviceGray").unwrap()),
            )
            .unwrap();
            assert_eq!(dib.format(), FxdibFormat::EightBppRgb);
            let bitmap = crate::fxge::dib::cfx_dibbase::convert_to(&dib, FxdibFormat::Bgr).unwrap();
            for y in 0..16 {
                let row = bitmap.scanline(y).unwrap();
                for x in 0..16 {
                    let value = x as u8 * 17;
                    let expected = if inverted { 255 - value } else { value };
                    assert_eq!(
                        &row[x * 3..x * 3 + 3],
                        &[expected; 3],
                        "explicit_space={explicit_space}, inverted={inverted}, x={x}, y={y}"
                    );
                }
            }
            assert_eq!(dib.palette().is_empty(), !inverted);
        }
    }

    #[test]
    fn jpx_rgb_samples_use_native_bgr_storage_and_borrow_cached_rows() {
        for (raw, bytes) in [
            (
                false,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/rgb.jp2"
                ))
                .as_slice(),
            ),
            (
                true,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/rgb.j2c"
                ))
                .as_slice(),
            ),
        ] {
            let mut dictionary = dict(16, 16, 0, if raw { b"DeviceRGB" } else { b"" });
            dictionary.set_new_name_for(b"Filter", b"JPXDecode");
            let stream = retain_object(CpdfStream::from_data_and_dict(
                bytes.to_vec(),
                retain_object(dictionary),
            ));
            let dib = CpdfDib::load(
                stream,
                if raw {
                    get_stock_cs_for_name(b"DeviceRGB")
                } else {
                    None
                },
            )
            .unwrap();
            assert_eq!(dib.format(), FxdibFormat::Bgr);
            assert!(dib.color_space().is_none());
            let row = dib.get_scanline(8);
            assert_eq!(&row[..3], &[0, 0, 255]);
            assert_eq!(&row[45..48], &[255, 0, 0]);
            // RGB JPX rows borrow the cached output instead of copying to the
            // DIB's scanline buffer.
            assert_eq!(
                row.as_ptr(),
                dib.cached_bitmap
                    .as_ref()
                    .unwrap()
                    .scanline(8)
                    .unwrap()
                    .as_ptr()
            );
        }
    }
    #[test]
    fn grayscale_jpx_embedded_alpha_preserves_gray_and_detaches_coverage() {
        for declared in [false, true] {
            for has_mask in [false, true] {
                let mut dictionary = dict(4, 4, 8, b"DeviceGray");
                if !declared {
                    dictionary.remove_for(b"ColorSpace");
                }
                dictionary.set_new_name_for(b"Filter", b"JPXDecode");
                dictionary.set_new_for(b"SMaskInData", CpdfScalarObject::number_from_signed(1));
                let stream = retain_object(CpdfStream::from_data_and_dict(
                    include_bytes!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../upstream/pdfium/testing/resources/gray-alpha.jp2"
                    ))
                    .to_vec(),
                    retain_object(dictionary),
                ));
                let data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
                let context = super::super::cpdf_image::CpdfImageLoadContext::new(&data, None);
                let dib = CpdfDib::load_with_context(
                    stream,
                    if declared {
                        get_stock_cs_for_name(b"DeviceGray")
                    } else {
                        None
                    },
                    context,
                    has_mask,
                )
                .unwrap();
                assert_eq!(&dib.get_scanline(1)[..4], &[0, 0, 255, 255]);
                let mask = dib.detach_mask();
                assert_eq!(mask.is_some(), has_mask);
                if let Some(mask) = mask {
                    let mask = mask.realize().unwrap();
                    assert_eq!(&mask.scanline(0).unwrap()[..4], &[255; 4]);
                    assert_eq!(&mask.scanline(1).unwrap()[..4], &[255, 128, 128, 255]);
                    assert_eq!(&mask.scanline(2).unwrap()[..4], &[255, 128, 128, 255]);
                }
                assert!(dib.detach_mask().is_none());
            }
        }
    }

    #[test]
    fn cmyk_jpx_embedded_alpha_preserves_four_color_samples_and_detaches_fifth_plane() {
        let expected_cmyk: [[u8; 16]; 4] = [
            [255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0],
            [255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0],
            [0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 255],
            [0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 0, 255],
        ];
        for declared in [false, true] {
            for has_mask in [false, true] {
                for smask_in_data in [0, 1] {
                    let mut dictionary = dict(4, 4, 8, b"DeviceCMYK");
                    if !declared {
                        dictionary.remove_for(b"ColorSpace");
                    }
                    dictionary.set_new_name_for(b"Filter", b"JPXDecode");
                    dictionary.set_new_for(
                        b"SMaskInData",
                        CpdfScalarObject::number_from_signed(smask_in_data),
                    );
                    let stream = retain_object(CpdfStream::from_data_and_dict(
                        include_bytes!(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/../../upstream/pdfium/testing/resources/CMYK-alpha.jpf"
                        ))
                        .to_vec(),
                        retain_object(dictionary),
                    ));
                    let data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
                    let context = super::super::cpdf_image::CpdfImageLoadContext::new(&data, None);
                    let dib = CpdfDib::load_with_context(
                        stream,
                        if declared {
                            get_stock_cs_for_name(b"DeviceCMYK")
                        } else {
                            None
                        },
                        context,
                        has_mask,
                    )
                    .unwrap();
                    assert_eq!((dib.width, dib.height, dib.components), (4, 4, 4));
                    for (row, samples) in expected_cmyk.iter().enumerate() {
                        assert_eq!(
                            &dib.cached_bitmap
                                .as_ref()
                                .unwrap()
                                .scanline(row as i32)
                                .unwrap()[..16],
                            samples,
                            "four CMYK samples must not be prematted or shifted by alpha"
                        );
                    }
                    assert_eq!(
                        &dib.get_scanline(1)[..12],
                        &[239, 174, 0, 239, 174, 0, 140, 2, 237, 140, 2, 237],
                    );
                    let mask = dib.detach_mask();
                    assert_eq!(mask.is_some(), has_mask && smask_in_data == 1);
                    if let Some(mask) = mask {
                        let mask = mask.realize().unwrap();
                        for row in 0..4 {
                            let coverage = if row == 1 || row == 2 {
                                [255, 128, 128, 255]
                            } else {
                                [255; 4]
                            };
                            assert_eq!(&mask.scanline(row).unwrap()[..4], &coverage);
                        }
                    }
                    assert!(dib.detach_mask().is_none());
                }
            }
        }
    }

    #[test]
    fn inline_jpx_alpha_keeps_straight_rgb_without_white_prematting() {
        let mut source = CfxDibitmap::create(4, 1, FxdibFormat::Bgra, 0).unwrap();
        source.scanline_mut(0).unwrap().copy_from_slice(&[
            10, 20, 30, 0, 40, 50, 60, 64, 70, 80, 90, 128, 100, 110, 120, 255,
        ]);
        for extract_mask in [false, true] {
            let (rgb, mask) =
                CpdfDib::convert_argb_jpx_bitmap_to_rgb(&source, extract_mask).unwrap();
            assert_eq!(
                rgb.scanline(0).unwrap(),
                &[10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120],
                "color samples must remain independent of opacity"
            );
            assert_eq!(mask.is_some(), extract_mask);
            if let Some(mask) = mask {
                assert_eq!(mask.scanline(0).unwrap(), &[0, 64, 128, 255]);
            }
        }
    }

    #[test]
    fn inline_jpx_alpha_is_detached_only_when_mask_loading_was_requested() {
        for has_mask in [false, true] {
            let mut dictionary = dict(16, 16, 0, b"DeviceRGB");
            dictionary.set_new_name_for(b"Filter", b"JPXDecode");
            dictionary.set_new_for(b"SMaskInData", CpdfScalarObject::number_from_signed(1));
            let stream = CpdfStream::from_data_and_dict(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/rgb.j2c"
                ))
                .to_vec(),
                retain_object(dictionary),
            );
            let data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
            let context = super::super::cpdf_image::CpdfImageLoadContext::new(&data, None);
            let dib = CpdfDib::load_with_context(
                retain_object(stream),
                get_stock_cs_for_name(b"DeviceRGB"),
                context,
                has_mask,
            )
            .unwrap();
            assert_eq!(dib.detach_mask().is_some(), has_mask);
            assert!(dib.detach_mask().is_none());
        }
    }

    #[test]
    fn jbig2_decode_failure_is_reported_by_continuation_and_releases_target() {
        let mut dictionary = dict(1, 1, 1, b"DeviceGray");
        dictionary.set_new_name_for(b"Filter", b"JBIG2Decode");
        let stream = retain_object(CpdfStream::from_data_and_dict(
            // A complete symbol-dictionary header with no required flag bytes.
            // A one-byte nonempty stream is a successful white page in C++.
            vec![0; 11],
            retain_object(dictionary),
        ));
        let (mut dib, state) = CpdfDib::start_load_with_max_size(
            stream,
            get_stock_cs_for_name(b"DeviceGray"),
            CfxSize::new(0, 0),
        )
        .unwrap();
        assert_eq!(state, LoadState::Continue);
        assert!(dib.cached_bitmap.is_some());
        assert_eq!(dib.continue_load(None), LoadState::Fail);
        assert!(dib.cached_bitmap.is_none());
        assert!(dib.global_acc.is_none());
    }
    #[test]
    fn standard_conversion_is_scoped_to_palette_loading() {
        let space = get_stock_cs_for_name(b"DeviceCMYK").unwrap();
        let data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
        let stream = || {
            retain_object(CpdfStream::from_data_and_dict(
                vec![0x80],
                retain_object(dict(1, 1, 1, b"DeviceCMYK")),
            ))
        };
        let mut context = super::super::cpdf_image::CpdfImageLoadContext::new(&data, None);
        context.std_cs = true;
        let standard =
            CpdfDib::load_with_context(stream(), Some(space.clone()), context, false).unwrap();
        assert_eq!(standard.palette()[1], 0xff00ffff);
        assert!(!space.data().is_std_conversion_enabled());
        context.std_cs = false;
        let adobe = CpdfDib::load_with_context(stream(), Some(space), context, false).unwrap();
        assert_ne!(adobe.palette()[1], standard.palette()[1]);
    }

    #[test]
    fn sixteen_bit_device_gray_matches_color_space_conversion_for_every_sample() {
        let samples: Vec<u8> = (0..=u16::MAX).flat_map(u16::to_be_bytes).collect();
        let space = get_stock_cs_for_name(b"DeviceGray").unwrap();
        for range in [
            [0.0, 1.0],
            [1.0, 0.0],
            [-0.5, 1.5],
            [0.17, 0.83],
            [0.5, 0.5],
        ] {
            let mut dictionary = dict(65536, 1, 16, b"DeviceGray");
            let mut decode = CpdfArray::new();
            for value in range {
                decode.append_new(CpdfScalarObject::number_from_float(value));
            }
            dictionary.set_new_for(b"Decode", decode);
            let dib = CpdfDib::load(
                retain_object(CpdfStream::from_data_and_dict(
                    samples.clone(),
                    retain_object(dictionary),
                )),
                Some(Rc::clone(&space)),
            )
            .unwrap();
            let mut expected = Vec::with_capacity(65536 * 3);
            for sample in 0..=u16::MAX {
                let value = range[0] + (range[1] - range[0]) / 65535.0 * f32::from(sample);
                let rgb = space.get_rgb_or_zeros_on_error(&[value]);
                expected.extend_from_slice(&[
                    (rgb.blue.clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb.green.clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb.red.clamp(0.0, 1.0) * 255.0) as u8,
                ]);
            }
            let mut actual = vec![0xa5; expected.len() + 7];
            dib.translate_scanline24bpp(&mut actual, &samples);
            assert_eq!(&actual[..expected.len()], &expected, "{range:?}");
            assert_eq!(&actual[expected.len()..], &[0xa5; 7]);
            assert_eq!(&*dib.get_scanline(0), &expected, "{range:?}");
        }
    }

    #[test]
    fn decoded_device_rgb_matches_color_space_conversion_for_all_byte_values() {
        let width = 257;
        let samples: Vec<u8> = (0..width)
            .flat_map(|value| [value as u8, (255 - value as u8), (value * 73) as u8])
            .collect();
        let space = get_stock_cs_for_name(b"DeviceRGB").unwrap();
        for ranges in [
            [0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
            [1.0, 0.0, 0.25, 0.75, -0.5, 1.5],
            [0.17, 0.83, 2.0, -1.0, 0.5, 0.5],
        ] {
            let mut dictionary = dict(width, 1, 8, b"DeviceRGB");
            let mut decode = CpdfArray::new();
            for value in ranges {
                decode.append_new(CpdfScalarObject::number_from_float(value));
            }
            dictionary.set_new_for(b"Decode", decode);
            let dib = CpdfDib::load(
                retain_object(CpdfStream::from_data_and_dict(
                    samples.clone(),
                    retain_object(dictionary),
                )),
                Some(Rc::clone(&space)),
            )
            .unwrap();
            let mut expected = Vec::new();
            for sample in samples.chunks_exact(3) {
                let values: [f32; 3] = std::array::from_fn(|channel| {
                    ranges[2 * channel]
                        + (ranges[2 * channel + 1] - ranges[2 * channel]) / 255.0
                            * f32::from(sample[channel])
                });
                let rgb = space.get_rgb_or_zeros_on_error(&values);
                expected.extend_from_slice(&[
                    (rgb.blue.clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb.green.clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb.red.clamp(0.0, 1.0) * 255.0) as u8,
                ]);
            }
            assert_eq!(&*dib.get_scanline(0), &expected, "{ranges:?}");
        }
    }

    #[test]
    fn cmyk_mask_conversion_keeps_default_and_decoded_channel_order() {
        let data = super::super::cpdf_docpagedata::CpdfDocPageData::new();
        for default_decode in [true, false] {
            let mut dictionary = dict(1, 1, 8, b"DeviceCMYK");
            if !default_decode {
                let mut decode = CpdfArray::new();
                for value in [0.0, 0.5, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0] {
                    decode.append_new(CpdfScalarObject::number_from_float(value));
                }
                dictionary.set_new_for(b"Decode", decode);
            }
            let stream = retain_object(CpdfStream::from_data_and_dict(
                vec![10, 20, 30, 40],
                retain_object(dictionary),
            ));
            let mut context = super::super::cpdf_image::CpdfImageLoadContext::new(&data, None);
            context.group_family = ColorSpaceFamily::DeviceCmyk;
            context.load_mask = true;
            let dib = CpdfDib::load_with_context(
                stream,
                get_stock_cs_for_name(b"DeviceCMYK"),
                context,
                false,
            )
            .unwrap();
            assert_eq!(
                &*dib.get_scanline(0),
                if default_decode {
                    &[206, 198, 189]
                } else {
                    &[189, 198, 210]
                }
            );
        }
    }
}

#[cfg(test)]
#[path = "cpdf_dib_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "icc_decode_tests.rs"]
mod icc_decode_tests;

#[path = "cpdf_dib_cache_inputs.rs"]
mod cache_inputs;
pub(crate) use cache_inputs::ImageCacheInputs;
