//! Symbol dictionary segment parsing and decoding (7.4.2, 6.5).

use alloc::vec;
use alloc::vec::Vec;

use crate::arithmetic_decoder::{ArithmeticDecoder, ArithmeticDecoderContext};
use crate::bitmap::{Bitmap, WORD_BITS};
use crate::decode::generic::{decode_bitmap_mmr, parse_adaptive_template_pixels};
use crate::decode::text::{
    DecodeContext, ReferenceCorner, TextRegionContexts, TextRegionFlags, TextRegionHeader,
    TextRegionHuffmanFlags, decode_with,
};
use crate::decode::{
    AdaptiveTemplatePixel, CombinationOperator, RefinementTemplate, RegionSegmentInfo, Template,
    parse_refinement_at_pixels,
};
use crate::decode::{generic, generic_refinement};
use crate::error::{
    DecodeError, HuffmanError, OverflowError, ParseError, Result, SymbolError, bail,
};
use crate::huffman_table::{HuffmanTable, StandardHuffmanTables, TableLine};
use crate::integer_decoder::IntegerDecoder;
use crate::reader::Reader;

const MAX_SYMBOLS: u32 = u16::MAX as u32;

/// Decode a symbol dictionary segment (7.4.2, 6.5).
pub(crate) fn decode(
    header: &SymbolDictionaryHeader<'_>,
    input_symbols: &[&Bitmap],
    referred_tables: &[HuffmanTable],
    standard_tables: &StandardHuffmanTables,
    input_contexts: Option<&RetainedContexts>,
) -> Result<SymbolDictionary> {
    let arithmetic = ArithmeticContext::new(header.data, header, input_contexts)?;
    decode_with_arithmetic(
        header,
        input_symbols,
        referred_tables,
        standard_tables,
        arithmetic,
    )
    .map(|(dictionary, _)| dictionary)
}

pub(crate) fn decode_with_initial_contexts<'a>(
    header: &'a SymbolDictionaryHeader<'a>,
    input_symbols: &'a [&'a Bitmap],
    referred_tables: &'a [HuffmanTable],
    standard_tables: &'a StandardHuffmanTables,
    contexts: RetainedContexts,
) -> Result<(SymbolDictionary, usize)> {
    let arithmetic = ArithmeticContext::with_contexts(header.data, contexts);
    decode_with_arithmetic(
        header,
        input_symbols,
        referred_tables,
        standard_tables,
        arithmetic,
    )
}

fn decode_with_arithmetic<'a>(
    header: &'a SymbolDictionaryHeader<'a>,
    input_symbols: &'a [&'a Bitmap],
    referred_tables: &'a [HuffmanTable],
    standard_tables: &'a StandardHuffmanTables,
    arithmetic: ArithmeticContext<'a>,
) -> Result<(SymbolDictionary, usize)> {
    let num_new_symbols = header.num_new_symbols;
    let mut ctx = SymbolDecodeContext {
        a_ctx: arithmetic,
        h_ctx: HuffmanContext::new(header.data, header, referred_tables, standard_tables)?,
        symbols: Symbols::new(input_symbols, num_new_symbols as usize),
        symbol_widths: Vec::with_capacity(num_new_symbols as usize),
        height_class_first_symbol: 0,
        symbols_decoded_count: 0,
        total_width: 0,
        height_class_height: 0,
        header,
        standard_tables,
    };

    let read_height_class_delta = |ctx: &mut SymbolDecodeContext<'_>| {
        if ctx.header.flags.use_huffman {
            ctx.h_ctx
                .height_class_delta_table
                .decode(&mut ctx.h_ctx.reader)
        } else {
            Ok(ctx
                .a_ctx
                .height_class_delta_decoder
                .decode(&mut ctx.a_ctx.decoder))
        }
    };

    let read_symbol_width_delta = |ctx: &mut SymbolDecodeContext<'_>| {
        if ctx.header.flags.use_huffman {
            ctx.h_ctx
                .symbol_width_delta_table
                .decode(&mut ctx.h_ctx.reader)
        } else {
            Ok(ctx
                .a_ctx
                .symbol_width_delta_decoder
                .decode(&mut ctx.a_ctx.decoder))
        }
    };

    while ctx.symbols_decoded_count < num_new_symbols {
        let height_delta = read_height_class_delta(&mut ctx)?;
        let height_class_delta = if header.pdfium && !header.flags.use_huffman {
            height_delta.unwrap_or(0)
        } else {
            height_delta.ok_or(SymbolError::UnexpectedOob)?
        };

        ctx.height_class_height = ctx
            .height_class_height
            .checked_add_signed(height_class_delta)
            .ok_or(OverflowError::BitmapDimension)?;

        if header.pdfium && ctx.height_class_height > u16::MAX as u32 {
            bail!(OverflowError::BitmapDimension);
        }
        let mut symbol_width: u32 = 0;
        ctx.total_width = 0;
        ctx.height_class_first_symbol = ctx.symbols_decoded_count;

        // "If the result of this decoding is OOB then all the symbols
        // in this height class have been decoded."
        while let Some(width_delta) = read_symbol_width_delta(&mut ctx)? {
            // Prevent infinite loop for invalid files.
            if ctx.symbols_decoded_count >= num_new_symbols {
                bail!(SymbolError::TooManySymbols);
            }

            symbol_width = symbol_width
                .checked_add_signed(width_delta)
                .ok_or(OverflowError::BitmapDimension)?;
            if header.pdfium && symbol_width > u16::MAX as u32 {
                bail!(OverflowError::BitmapDimension);
            }
            ctx.total_width = if header.pdfium {
                ctx.total_width.wrapping_add(symbol_width)
            } else {
                ctx.total_width
                    .checked_add(symbol_width)
                    .ok_or(OverflowError::BitmapDimension)?
            };
            if header.pdfium && (ctx.height_class_height == 0 || symbol_width == 0) {
                if header.flags.use_huffman && !header.flags.use_refagg {
                    // Native width slots start zero and remain zero on this branch.
                    ctx.symbol_widths.push(0);
                } else {
                    ctx.symbols.new.push(Bitmap::new_null());
                }
                ctx.symbols_decoded_count += 1;
                continue;
            }

            match (ctx.header.flags.use_huffman, ctx.header.flags.use_refagg) {
                (false, false) => {
                    // Decode a single symbol using a simple generic decoding procedure,
                    // described in 6.5.8.1.
                    let mut region = Bitmap::new_region(
                        symbol_width,
                        ctx.height_class_height,
                        0,
                        0,
                        false,
                        ctx.header.pdfium,
                    )?;
                    generic::decode_bitmap_arithmetic_coding(
                        &mut region,
                        &mut ctx.a_ctx.decoder,
                        &mut ctx.a_ctx.generic_region_contexts,
                        ctx.header.flags.template,
                        false,
                        &ctx.header.adaptive_template_pixels,
                    )?;

                    ctx.symbols.new.push(region);
                }
                (true, false) => {
                    // Decode a single symbol width. We don't actually decode the symbols
                    // yet, those will be decoded later on from the collective bitmap.
                    ctx.symbol_widths.push(symbol_width);
                }
                (_, true) => {
                    // Also decode a single symbol, but using refinement-aggregation.
                    // In this case, we can have both, huffman and arithmetic coding.
                    let symbol = decode_refinement_aggregation_bitmap(&mut ctx, symbol_width)?;

                    ctx.symbols.new.push(symbol);
                }
            }

            ctx.symbols_decoded_count += 1;
        }

        if ctx.header.flags.use_huffman && !ctx.header.flags.use_refagg {
            // In case we have huffman coding and no refinement-aggregation, we use
            // the previously decoded symbol widths to decode the collective bitmap
            // and extract the individual symbols from that bitmap.
            decode_height_class_collective_bitmap(&mut ctx)?;
        }
    }

    let exported = export_symbols(&mut ctx)?;
    let consumed = if header.flags.use_huffman {
        (ctx.h_ctx.reader.byte_pos() + usize::from(ctx.h_ctx.reader.bit_pos() != 0))
            .min(header.data.len())
    } else {
        ctx.a_ctx
            .decoder
            .byte_position()
            .saturating_add(2)
            .min(header.data.len())
    };

    let retained_contexts = if ctx.header.flags.bitmap_context_retained {
        Some(RetainedContexts {
            generic_region: ctx.a_ctx.generic_region_contexts,
            refinement_region: ctx.a_ctx.refinement_region_contexts,
        })
    } else {
        None
    };

    Ok((
        SymbolDictionary {
            exported_symbols: exported,
            retained_contexts,
        },
        consumed,
    ))
}

#[derive(Debug, Clone)]
pub(crate) struct RetainedContexts {
    pub(crate) generic_region: Vec<ArithmeticDecoderContext>,
    pub(crate) refinement_region: Vec<ArithmeticDecoderContext>,
}

// Copyright 2014 The PDFium Authors. Original code copyright 2014 Foxit
// Software Inc. http://www.foxitsoftware.com
// PDFium context/cache extensions use the BSD-style license in
// ../../LICENSE-PDFIUM and follow CJBig2_Context::ParseSymbolDict.
// PDFium constructs and validates these contexts before cache lookup, and
// attaches the initial (rather than decoded) contexts after a cache hit.
pub(crate) fn initial_contexts(
    header: &SymbolDictionaryHeader<'_>,
    last_dictionary: Option<&SymbolDictionary>,
) -> Result<RetainedContexts> {
    fn context_array(
        needed: bool,
        count: usize,
        use_previous: bool,
        last_dictionary: Option<&SymbolDictionary>,
        refinement: bool,
    ) -> Result<Vec<ArithmeticDecoderContext>> {
        if !needed {
            return Ok(Vec::new());
        }
        if use_previous && last_dictionary.is_some() {
            let previous = last_dictionary.and_then(|dict| dict.retained_contexts.as_ref());
            let values = previous
                .map(|previous| {
                    if refinement {
                        &previous.refinement_region[..]
                    } else {
                        &previous.generic_region[..]
                    }
                })
                .unwrap_or(&[]);
            if values.len() != count {
                bail!(SymbolError::Invalid);
            }
            return Ok(values.to_vec());
        }
        Ok(vec![ArithmeticDecoderContext::default(); count])
    }
    Ok(RetainedContexts {
        generic_region: context_array(
            !header.flags.use_huffman,
            1 << header.flags.template.context_bits(),
            header.flags.bitmap_context_used,
            last_dictionary,
            false,
        )?,
        refinement_region: context_array(
            header.flags.use_refagg,
            1 << header.flags.refinement_template.context_bits(),
            header.flags.bitmap_context_used,
            last_dictionary,
            true,
        )?,
    })
}

/// A decoded symbol dictionary segment.
#[derive(Debug, Clone)]
pub(crate) struct SymbolDictionary {
    pub(crate) exported_symbols: Vec<Bitmap>,
    pub(crate) retained_contexts: Option<RetainedContexts>,
}

/// Decode a symbol bitmap using refinement/aggregate coding (6.5.8.2).
fn decode_refinement_aggregation_bitmap(
    ctx: &mut SymbolDecodeContext<'_>,
    symbol_width: u32,
) -> Result<Bitmap> {
    // 6.5.8.2.1 Number of symbol instances in the aggregation.
    let aggregation_instance_count = if ctx.header.flags.use_huffman {
        ctx.h_ctx
            .aggregation_instance_count_table
            .decode(&mut ctx.h_ctx.reader)?
    } else {
        ctx.a_ctx
            .aggregation_instance_count_decoder
            .decode(&mut ctx.a_ctx.decoder)
    };
    let aggregation_instance_count = if ctx.header.pdfium && !ctx.header.flags.use_huffman {
        aggregation_instance_count.unwrap_or(0)
    } else {
        aggregation_instance_count.ok_or(SymbolError::UnexpectedOob)?
    };
    if ctx.header.pdfium {
        let count = aggregation_instance_count as u32;
        return match count {
            0 => Ok(Bitmap::new_null()),
            1 => decode_refinement_bitmap(ctx, symbol_width),
            _ => decode_aggregation_bitmap(ctx, symbol_width, count),
        };
    }

    if aggregation_instance_count == 1 {
        decode_refinement_bitmap(ctx, symbol_width)
    } else if aggregation_instance_count > 1 {
        decode_aggregation_bitmap(ctx, symbol_width, aggregation_instance_count as u32)
    } else {
        Err(DecodeError::Symbol(SymbolError::Invalid))
    }
}

/// Decode a refinement bitmap symbol with a single aggregate (6.5.8.2).
fn decode_refinement_bitmap(
    ctx: &mut SymbolDecodeContext<'_>,
    symbol_width: u32,
) -> Result<Bitmap> {
    let use_huffman = ctx.header.flags.use_huffman;
    let mut symbol_code_length = 32 - (ctx.total_symbols() - 1).leading_zeros();

    let (symbol_id, refinement_x_offset, refinement_y_offset) = if use_huffman {
        // See 6.5.8.2.3, the value should be at least 1 if we use huffman coding.
        symbol_code_length = symbol_code_length.max(1);

        let symbol_id = ctx
            .h_ctx
            .reader
            .read_bits(symbol_code_length as u8)
            .ok_or(ParseError::UnexpectedEof)? as usize;

        let refinement_x_offset = ctx
            .standard_tables
            .table_o()
            .decode(&mut ctx.h_ctx.reader)?
            .ok_or(HuffmanError::UnexpectedOob)?;

        let refinement_y_offset = ctx
            .standard_tables
            .table_o()
            .decode(&mut ctx.h_ctx.reader)?
            .ok_or(HuffmanError::UnexpectedOob)?;

        (symbol_id, refinement_x_offset, refinement_y_offset)
    } else {
        // Note that the contexts should be reused across multiple
        // bitmaps in the same symbol dictionary.
        let contexts = ctx
            .a_ctx
            .text_region_contexts
            .get_or_insert_with(|| TextRegionContexts::new(symbol_code_length));

        let symbol_id = contexts.iaid.decode(&mut ctx.a_ctx.decoder) as usize;

        let decoded_x = contexts.iardx.decode(&mut ctx.a_ctx.decoder);
        let refinement_x_offset = if ctx.header.pdfium {
            decoded_x.unwrap_or(0)
        } else {
            decoded_x.ok_or(SymbolError::UnexpectedOob)?
        };

        let decoded_y = contexts.iardy.decode(&mut ctx.a_ctx.decoder);
        let refinement_y_offset = if ctx.header.pdfium {
            decoded_y.unwrap_or(0)
        } else {
            decoded_y.ok_or(SymbolError::UnexpectedOob)?
        };

        (symbol_id, refinement_x_offset, refinement_y_offset)
    };

    let reference_region = ctx.symbols.get(symbol_id).ok_or(SymbolError::OutOfRange)?;
    if ctx.header.pdfium && reference_region.is_null {
        bail!(SymbolError::Invalid);
    }
    let mut region = Bitmap::new_region(
        symbol_width,
        ctx.height_class_height,
        0,
        0,
        false,
        ctx.header.pdfium,
    )?;

    if use_huffman && ctx.header.pdfium {
        let size = ctx
            .standard_tables
            .table_a()
            .decode(&mut ctx.h_ctx.reader)?
            .ok_or(HuffmanError::UnexpectedOob)? as u32;
        ctx.h_ctx.reader.align();
        let data = ctx.h_ctx.reader.tail().ok_or(ParseError::UnexpectedEof)?;
        let mut decoder = ArithmeticDecoder::new_with_pdfium(data, true);
        generic_refinement::decode_bitmap(
            &mut decoder,
            &mut ctx.a_ctx.refinement_region_contexts,
            &mut region,
            reference_region,
            refinement_x_offset,
            refinement_y_offset,
            ctx.header.flags.refinement_template,
            &ctx.header.refinement_at_pixels,
            false,
        )?;
        let consumed = decoder.byte_position().saturating_add(2).min(data.len());
        ctx.h_ctx
            .reader
            .skip_bytes(consumed)
            .ok_or(ParseError::UnexpectedEof)?;
        if size as usize != consumed {
            bail!(HuffmanError::InvalidCode);
        }
    } else if use_huffman {
        let bitmap_size = ctx
            .standard_tables
            .table_a()
            .decode(&mut ctx.h_ctx.reader)?
            .ok_or(HuffmanError::UnexpectedOob)? as usize;
        ctx.h_ctx.reader.align();

        let bitmap_data = ctx
            .h_ctx
            .reader
            .read_bytes(bitmap_size)
            .ok_or(ParseError::UnexpectedEof)?;

        let mut bitmap_decoder = ArithmeticDecoder::new_with_pdfium(bitmap_data, ctx.header.pdfium);

        generic_refinement::decode_bitmap(
            &mut bitmap_decoder,
            &mut ctx.a_ctx.refinement_region_contexts,
            &mut region,
            reference_region,
            refinement_x_offset,
            refinement_y_offset,
            ctx.header.flags.refinement_template,
            &ctx.header.refinement_at_pixels,
            false,
        )?;
    } else {
        generic_refinement::decode_bitmap(
            &mut ctx.a_ctx.decoder,
            &mut ctx.a_ctx.refinement_region_contexts,
            &mut region,
            reference_region,
            refinement_x_offset,
            refinement_y_offset,
            ctx.header.flags.refinement_template,
            &ctx.header.refinement_at_pixels,
            false,
        )?;
    }

    Ok(region)
}

/// Decode an aggregation bitmap with more than one aggregate (6.5.8.2).
fn decode_aggregation_bitmap(
    ctx: &mut SymbolDecodeContext<'_>,
    symbol_width: u32,
    aggregation_instance_count: u32,
) -> Result<Bitmap> {
    let use_huffman = ctx.header.flags.use_huffman;

    // Concatenate input and new symbols.
    let mut all_symbols: Vec<&Bitmap> =
        Vec::with_capacity(ctx.symbols.input.len() + ctx.symbols.new.len());
    all_symbols.extend(ctx.symbols.input.iter().copied());
    for sym in &ctx.symbols.new {
        all_symbols.push(sym);
    }

    // Set all parameters according to Table 17.

    let mut symbol_code_length = 32 - (ctx.total_symbols() - 1).leading_zeros();
    if use_huffman && ctx.header.pdfium {
        symbol_code_length = symbol_code_length.max(1);
    }

    let symbol_id_table = if use_huffman {
        Some(if ctx.header.pdfium {
            // SBNUMSYMS includes input and already decoded symbols. Future
            // symbols affect SBSYMCODELEN, but have no entries in SBSYMCODES.
            HuffmanTable::build_pdfium(
                (0..all_symbols.len())
                    .map(|id| TableLine::new(id as i32, symbol_code_length as u8, 0))
                    .collect(),
            )?
        } else {
            HuffmanTable::build_uniform(ctx.total_symbols(), symbol_code_length)?
        })
    } else {
        None
    };

    let huffman_flags = if use_huffman {
        Some(TextRegionHuffmanFlags {
            first_s_table: 0,
            delta_s_table: 0,
            delta_t_table: 0,
            refinement_width_table: 1,
            refinement_height_table: 1,
            refinement_y_table: 1,
            refinement_x_table: 1,
            refinement_size_table: 0,
        })
    } else {
        None
    };

    let header = TextRegionHeader {
        pdfium: ctx.header.pdfium,
        region_info: RegionSegmentInfo {
            width: symbol_width,
            height: ctx.height_class_height,
            x_location: 0,
            y_location: 0,
            combination_operator: CombinationOperator::Or,
            _colour_extension: false,
        },
        flags: TextRegionFlags {
            use_huffman,
            use_refinement: true,
            log_strip_size: 0,
            reference_corner: ReferenceCorner::TopLeft,
            transposed: false,
            combination_operator: CombinationOperator::Or,
            default_pixel: false,
            delta_s_offset: 0,
            refinement_template: ctx.header.flags.refinement_template,
        },
        huffman_flags,
        refinement_at_pixels: ctx.header.refinement_at_pixels.clone(),
        num_instances: aggregation_instance_count,
        symbol_id_table,
        // The native context's stream-size check applies to top-level text
        // segments, not to symbol aggregation's internal TRD invocation.
        segment_data_len: ctx.header.data.len(),
        check_instance_count: !ctx.header.pdfium,
        data: &[], // Data comes from parent decoder context, not the header.
    };

    let decode_ctx = if use_huffman {
        let gr_contexts = if ctx.header.pdfium {
            &mut ctx.a_ctx.refinement_region_contexts[..]
        } else {
            &mut []
        };
        DecodeContext::new_huffman(
            &mut ctx.h_ctx.reader,
            &header,
            &[],
            ctx.standard_tables,
            gr_contexts,
        )?
    } else {
        let contexts = ctx
            .a_ctx
            .text_region_contexts
            .get_or_insert_with(|| TextRegionContexts::new(symbol_code_length));

        DecodeContext::new_arithmetic(
            &mut ctx.a_ctx.decoder,
            contexts,
            &mut ctx.a_ctx.refinement_region_contexts,
        )
    };

    let mut bitmap = Bitmap::new_region(
        header.region_info.width,
        header.region_info.height,
        header.region_info.x_location,
        header.region_info.y_location,
        header.flags.default_pixel,
        header.pdfium,
    )?;
    bitmap.pdfium = header.pdfium;
    decode_with(decode_ctx, &all_symbols, &header, &mut bitmap)?;
    Ok(bitmap)
}

struct Symbols<'a> {
    input: &'a [&'a Bitmap],
    new: Vec<Bitmap>,
}

impl<'a> Symbols<'a> {
    fn new(input: &'a [&'a Bitmap], capacity: usize) -> Self {
        Self {
            input,
            new: Vec::with_capacity(capacity),
        }
    }

    fn input_count(&self) -> u32 {
        self.input.len() as u32
    }

    fn get(&self, index: usize) -> Option<&Bitmap> {
        if index < self.input.len() {
            Some(self.input[index])
        } else {
            self.new.get(index - self.input.len())
        }
    }
}

struct SymbolDecodeContext<'a> {
    header: &'a SymbolDictionaryHeader<'a>,
    a_ctx: ArithmeticContext<'a>,
    h_ctx: HuffmanContext<'a>,
    symbols: Symbols<'a>,
    standard_tables: &'a StandardHuffmanTables,
    symbol_widths: Vec<u32>,
    height_class_first_symbol: u32,
    symbols_decoded_count: u32,
    total_width: u32,
    height_class_height: u32,
}

impl SymbolDecodeContext<'_> {
    fn total_symbols(&self) -> u32 {
        self.symbols.input_count() + self.header.num_new_symbols
    }
}

struct ArithmeticContext<'a> {
    decoder: ArithmeticDecoder<'a>,
    /// `IADH`
    height_class_delta_decoder: IntegerDecoder,
    /// `IADW`
    symbol_width_delta_decoder: IntegerDecoder,
    /// `IAEX`
    export_run_length_decoder: IntegerDecoder,
    /// `IAAI`
    aggregation_instance_count_decoder: IntegerDecoder,
    generic_region_contexts: Vec<ArithmeticDecoderContext>,
    refinement_region_contexts: Vec<ArithmeticDecoderContext>,
    text_region_contexts: Option<TextRegionContexts>,
}

impl<'a> ArithmeticContext<'a> {
    fn with_contexts(data: &'a [u8], contexts: RetainedContexts) -> Self {
        Self {
            decoder: ArithmeticDecoder::new_with_pdfium(data, true),
            height_class_delta_decoder: IntegerDecoder::new(),
            symbol_width_delta_decoder: IntegerDecoder::new(),
            export_run_length_decoder: IntegerDecoder::new(),
            aggregation_instance_count_decoder: IntegerDecoder::new(),
            generic_region_contexts: contexts.generic_region,
            refinement_region_contexts: contexts.refinement_region,
            text_region_contexts: None,
        }
    }
    fn new(
        data: &'a [u8],
        header: &SymbolDictionaryHeader<'_>,
        input_contexts: Option<&RetainedContexts>,
    ) -> Result<Self> {
        let decoder = ArithmeticDecoder::new_with_pdfium(data, header.pdfium);

        let height_class_delta_decoder = IntegerDecoder::new();
        let symbol_width_delta_decoder = IntegerDecoder::new();
        let export_run_length_decoder = IntegerDecoder::new();
        let aggregation_instance_count_decoder = IntegerDecoder::new();

        let (generic_region_contexts, refinement_region_contexts) =
            if header.flags.bitmap_context_used {
                let ctx = input_contexts.ok_or(SymbolError::Invalid)?;
                (ctx.generic_region.clone(), ctx.refinement_region.clone())
            } else {
                let template = header.flags.template;
                let num_contexts = 1 << template.context_bits();
                let refinement_template = header.flags.refinement_template;
                let num_refinement_contexts = 1 << refinement_template.context_bits();
                (
                    vec![ArithmeticDecoderContext::default(); num_contexts],
                    vec![ArithmeticDecoderContext::default(); num_refinement_contexts],
                )
            };

        Ok(Self {
            decoder,
            height_class_delta_decoder,
            symbol_width_delta_decoder,
            export_run_length_decoder,
            aggregation_instance_count_decoder,
            generic_region_contexts,
            refinement_region_contexts,
            text_region_contexts: None,
        })
    }
}

struct HuffmanContext<'a> {
    /// `SDHUFFDH`
    height_class_delta_table: &'a HuffmanTable,
    /// `SDHUFFDW`
    symbol_width_delta_table: &'a HuffmanTable,
    /// `SDHUFFBMSIZE`
    collective_bitmap_size_table: &'a HuffmanTable,
    /// `SDHUFFAGGINST`
    aggregation_instance_count_table: &'a HuffmanTable,
    export_run_length_table: &'a HuffmanTable,
    reader: Reader<'a>,
}

impl<'a> HuffmanContext<'a> {
    fn new(
        data: &'a [u8],
        header: &SymbolDictionaryHeader<'_>,
        referred_tables: &'a [HuffmanTable],
        standard_tables: &'a StandardHuffmanTables,
    ) -> Result<Self> {
        let reader = if header.pdfium {
            Reader::new_pdfium(data)
        } else {
            Reader::new(data)
        };

        let custom_count = [
            header.flags.delta_height_table == HuffmanTableSelection::UserSupplied,
            header.flags.delta_width_table == HuffmanTableSelection::UserSupplied,
            header.flags.collective_bitmap_size_table == HuffmanTableSelection::UserSupplied,
            header.flags.aggregate_instance_table == HuffmanTableSelection::UserSupplied,
        ]
        .into_iter()
        .filter(|x| *x)
        .count();

        if referred_tables.len() < custom_count {
            bail!(HuffmanError::MissingTables);
        }

        let mut custom_table_idx = 0;
        let mut get_custom = || -> &HuffmanTable {
            let table = &referred_tables[custom_table_idx];
            custom_table_idx += 1;
            table
        };

        // Select Huffman tables based on flags (7.4.2.1.6).
        let mut get_table = |selection: HuffmanTableSelection| -> &HuffmanTable {
            match selection {
                HuffmanTableSelection::TableB1 => standard_tables.table_a(),
                HuffmanTableSelection::TableB2 => standard_tables.table_b(),
                HuffmanTableSelection::TableB3 => standard_tables.table_c(),
                HuffmanTableSelection::TableB4 => standard_tables.table_d(),
                HuffmanTableSelection::TableB5 => standard_tables.table_e(),
                HuffmanTableSelection::UserSupplied => get_custom(),
            }
        };

        let height_class_delta_table = get_table(header.flags.delta_height_table);
        let symbol_width_delta_table = get_table(header.flags.delta_width_table);
        let collective_bitmap_size_table = get_table(header.flags.collective_bitmap_size_table);
        let aggregation_instance_count_table = get_table(header.flags.aggregate_instance_table);
        let export_run_length_table = get_table(HuffmanTableSelection::TableB1);

        Ok(Self {
            reader,
            height_class_delta_table,
            symbol_width_delta_table,
            collective_bitmap_size_table,
            aggregation_instance_count_table,
            export_run_length_table,
        })
    }
}

/// Decode a height class collective bitmap (6.5.9).
fn decode_height_class_collective_bitmap(ctx: &mut SymbolDecodeContext<'_>) -> Result<()> {
    let bitmap_size = ctx
        .h_ctx
        .collective_bitmap_size_table
        .decode(&mut ctx.h_ctx.reader)?
        .ok_or(HuffmanError::UnexpectedOob)?;
    let bitmap_size = if ctx.header.pdfium {
        bitmap_size as u32
    } else {
        u32::try_from(bitmap_size).map_err(|_| OverflowError::BitmapDimension)?
    };

    ctx.h_ctx.reader.align();

    let collective_bitmap = if bitmap_size == 0 {
        // Bitmap is stored uncompressed, so we basically just copy-paste
        // each byte. Rows are padded to the byte boundary.
        let row_bytes = ctx.total_width.div_ceil(8);

        let mut bitmap = if ctx.header.pdfium {
            if (ctx.total_width as i32) > i32::from(u16::MAX) {
                bail!(OverflowError::BitmapDimension);
            }
            let size = row_bytes
                .checked_mul(ctx.height_class_height)
                .ok_or(OverflowError::BitmapDimension)?;
            if ctx.h_ctx.reader.tail().map_or(0, |data| data.len()) < size as usize {
                bail!(ParseError::UnexpectedEof);
            }
            let image = Bitmap::new_pdfium(ctx.total_width, ctx.height_class_height);
            if size != 0 && image.data.is_empty() {
                bail!(OverflowError::BitmapDimension);
            }
            image
        } else {
            Bitmap::new(ctx.total_width, ctx.height_class_height)?
        };
        for y in 0..ctx.height_class_height {
            for byte_x in 0..row_bytes {
                let byte = ctx
                    .h_ctx
                    .reader
                    .read_byte()
                    .ok_or(ParseError::UnexpectedEof)?;
                if ctx.header.pdfium {
                    let index = (y * bitmap.stride + byte_x / 4) as usize;
                    bitmap.data[index] |= u32::from(byte) << (24 - (byte_x % 4) * 8);
                    continue;
                }
                for bit in 0..8 {
                    let x = byte_x * 8 + bit;
                    if x < ctx.total_width {
                        let pixel = (byte >> (7 - bit)) & 1;
                        bitmap.set_pixel(x, y, pixel);
                    }
                }
            }
        }
        bitmap
    } else if ctx.header.pdfium {
        // Native BMSIZE selects MMR without bounding the source by that value.
        // The following code starts where FaxG4Decode actually stopped.
        let mut bitmap = Bitmap::new_pdfium(ctx.total_width, ctx.height_class_height);
        if bitmap.data.is_empty() {
            // Native leaves this class's preallocated symbol slots null and
            // continues after failed MMR image construction without reading pixels.
            for _ in ctx.height_class_first_symbol..ctx.symbols_decoded_count {
                ctx.symbols.new.push(Bitmap::new_null());
            }
            return Ok(());
        }
        let data = ctx.h_ctx.reader.tail().ok_or(ParseError::UnexpectedEof)?;
        let consumed = decode_bitmap_mmr(&mut bitmap, data, true)?;
        ctx.h_ctx
            .reader
            .skip_bytes(consumed)
            .ok_or(ParseError::UnexpectedEof)?;
        bitmap
    } else {
        // Otherwise, we need to use MMR decoding.
        let bitmap_data = ctx
            .h_ctx
            .reader
            .read_bytes(bitmap_size as usize)
            .ok_or(ParseError::UnexpectedEof)?;

        let mut bitmap = Bitmap::new(ctx.total_width, ctx.height_class_height)?;
        decode_bitmap_mmr(&mut bitmap, bitmap_data, false)?;
        bitmap
    };

    if ctx.header.pdfium {
        let mut x = 0u32;
        for index in ctx.height_class_first_symbol..ctx.symbols_decoded_count {
            let width = ctx.symbol_widths[index as usize];
            ctx.symbols.new.push(collective_bitmap.pdfium_subimage(
                x as i32,
                0,
                width,
                ctx.height_class_height,
            )?);
            x = x.wrapping_add(width);
        }
        return Ok(());
    }

    // Finally, we simply chop up the collective bitmap into its constituent
    // symbols.
    let mut x_offset: u32 = 0;
    for symbol_idx in ctx.height_class_first_symbol..ctx.symbols_decoded_count {
        let symbol_width = ctx.symbol_widths[symbol_idx as usize];
        let mut symbol = Bitmap::new(symbol_width, ctx.height_class_height)?;

        for y in 0..ctx.height_class_height {
            let src_bit_offset = x_offset % WORD_BITS;
            let src_word_start = x_offset / WORD_BITS;
            let dest_row = (y * symbol.stride) as usize;

            for w in 0..symbol.stride {
                let sw = src_word_start + w;
                let src1 = collective_bitmap.get_word(y, sw);
                let word = if src_bit_offset == 0 {
                    src1
                } else {
                    let src2 = collective_bitmap.get_word(y, sw + 1);
                    (src1 << src_bit_offset) | (src2 >> (WORD_BITS - src_bit_offset))
                };
                symbol.data[dest_row + w as usize] = word;
            }

            // Clear trailing bits in the last word beyond `symbol_width`.
            let trailing = symbol_width % WORD_BITS;
            if trailing != 0 && symbol.stride > 0 {
                let last = dest_row + (symbol.stride - 1) as usize;
                symbol.data[last] &= !0 << (WORD_BITS - trailing);
            }
        }

        ctx.symbols.new.push(symbol);
        x_offset = x_offset
            .checked_add(symbol_width)
            .ok_or(OverflowError::Index)?;
    }

    Ok(())
}

/// Native SDD export flags. Input images use the source's explicit deep copy;
/// newly decoded images transfer ownership and unexported ones are dropped.
fn export_symbols_pdfium(ctx: &mut SymbolDecodeContext<'_>) -> Result<Vec<Bitmap>> {
    let total = ctx.total_symbols();
    let mut flags = vec![false; total as usize];
    let mut index = 0u32;
    let mut current = false;
    let mut count = 0u32;
    while index < total {
        let value = if ctx.header.flags.use_huffman {
            ctx.h_ctx
                .export_run_length_table
                .decode(&mut ctx.h_ctx.reader)?
                .ok_or(HuffmanError::UnexpectedOob)?
        } else {
            // Native IAEX ignores status; OOB and overflow write zero.
            ctx.a_ctx
                .export_run_length_decoder
                .decode(&mut ctx.a_ctx.decoder)
                .unwrap_or(0)
        };
        let run = value as u32;
        let end = index.checked_add(run).ok_or(OverflowError::Index)?;
        if end > total {
            bail!(SymbolError::OutOfRange);
        }
        if current {
            count += run;
        }
        flags[index as usize..end as usize].fill(current);
        index = end;
        current = !current;
    }
    if count > ctx.header.num_exported_symbols {
        bail!(SymbolError::TooManySymbols);
    }
    if ctx.symbols.new.len() != ctx.header.num_new_symbols as usize {
        bail!(SymbolError::OutOfRange);
    }
    let mut exported = Vec::with_capacity(count as usize);
    let input_count = ctx.symbols.input.len();
    for (image, &flag) in ctx.symbols.input.iter().zip(&flags[..input_count]) {
        if flag {
            exported.push((**image).clone());
        }
    }
    for (image, &flag) in core::mem::take(&mut ctx.symbols.new)
        .into_iter()
        .zip(&flags[input_count..])
    {
        if flag {
            exported.push(image);
        }
    }
    Ok(exported)
}

/// Exported symbols (6.5.10).
fn export_symbols(ctx: &mut SymbolDecodeContext<'_>) -> Result<Vec<Bitmap>> {
    if ctx.header.pdfium {
        return export_symbols_pdfium(ctx);
    }
    let total_symbols = ctx.total_symbols();

    let mut read_run_length = || -> Result<u32> {
        let value = if ctx.header.flags.use_huffman {
            ctx.h_ctx
                .export_run_length_table
                .decode(&mut ctx.h_ctx.reader)?
        } else {
            ctx.a_ctx
                .export_run_length_decoder
                .decode(&mut ctx.a_ctx.decoder)
        }
        .ok_or(HuffmanError::UnexpectedOob)?;

        u32::try_from(value).map_err(|_| HuffmanError::InvalidCode.into())
    };
    let mut exported = Vec::with_capacity(ctx.header.num_exported_symbols as usize);
    let mut index: u32 = 0;
    let mut should_export = false;

    while index < total_symbols {
        let run_length = read_run_length()?;

        let end_index = index.checked_add(run_length).ok_or(OverflowError::Index)?;
        if end_index > total_symbols {
            bail!(SymbolError::OutOfRange);
        }

        if should_export {
            for symbol_idx in index..end_index {
                let symbol = ctx
                    .symbols
                    .get(symbol_idx as usize)
                    .ok_or(SymbolError::OutOfRange)?
                    .clone();
                exported.push(symbol);
            }
        }

        index = end_index;
        should_export = !should_export;
    }

    if exported.len() != ctx.header.num_exported_symbols as usize {
        bail!(SymbolError::NoSymbols);
    }

    Ok(exported)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HuffmanTableSelection {
    TableB1,
    TableB2,
    TableB3,
    TableB4,
    TableB5,
    UserSupplied,
}

/// Parsed symbol dictionary segment flags (7.4.2.1.1).
#[derive(Debug, Clone)]
pub(crate) struct SymbolDictionaryFlags {
    pub(crate) use_huffman: bool,
    pub(crate) use_refagg: bool,
    pub(crate) delta_height_table: HuffmanTableSelection,
    pub(crate) delta_width_table: HuffmanTableSelection,
    pub(crate) collective_bitmap_size_table: HuffmanTableSelection,
    pub(crate) aggregate_instance_table: HuffmanTableSelection,
    pub(crate) bitmap_context_used: bool,
    pub(crate) bitmap_context_retained: bool,
    pub(crate) template: Template,
    pub(crate) refinement_template: RefinementTemplate,
}

/// Parsed symbol dictionary segment header (7.4.2.1).
#[derive(Debug, Clone)]
pub(crate) struct SymbolDictionaryHeader<'a> {
    pub(crate) pdfium: bool,
    pub(crate) flags: SymbolDictionaryFlags,
    pub(crate) adaptive_template_pixels: [AdaptiveTemplatePixel; 4],
    pub(crate) refinement_at_pixels: Vec<AdaptiveTemplatePixel>,
    pub(crate) num_exported_symbols: u32,
    pub(crate) num_new_symbols: u32,
    pub(crate) data: &'a [u8],
}

/// Parse a symbol dictionary segment header (7.4.2.1).
pub(crate) fn parse<'a>(reader: &mut Reader<'a>) -> Result<SymbolDictionaryHeader<'a>> {
    let flags_word = reader.read_u16().ok_or(ParseError::UnexpectedEof)?;
    let use_huffman = flags_word & 0x0001 != 0;
    let use_refagg = flags_word & 0x0002 != 0;

    let delta_height_table = if reader.is_pdfium() && !use_huffman {
        HuffmanTableSelection::TableB4
    } else {
        match (flags_word >> 2) & 0x03 {
            0 => HuffmanTableSelection::TableB4,
            1 => HuffmanTableSelection::TableB5,
            3 => HuffmanTableSelection::UserSupplied,
            _ => bail!(HuffmanError::InvalidSelection),
        }
    };

    let delta_width_table = if reader.is_pdfium() && !use_huffman {
        HuffmanTableSelection::TableB2
    } else {
        match (flags_word >> 4) & 0x03 {
            0 => HuffmanTableSelection::TableB2,
            1 => HuffmanTableSelection::TableB3,
            3 => HuffmanTableSelection::UserSupplied,
            _ => bail!(HuffmanError::InvalidSelection),
        }
    };

    let collective_bitmap_size_table =
        if flags_word & 0x0040 != 0 && (!reader.is_pdfium() || use_huffman) {
            HuffmanTableSelection::UserSupplied
        } else {
            HuffmanTableSelection::TableB1
        };

    let aggregate_instance_table =
        if flags_word & 0x0080 != 0 && (!reader.is_pdfium() || (use_huffman && use_refagg)) {
            HuffmanTableSelection::UserSupplied
        } else {
            HuffmanTableSelection::TableB1
        };

    let bitmap_context_used = flags_word & 0x0100 != 0;
    let bitmap_context_retained = flags_word & 0x0200 != 0;
    let template = Template::from_byte((flags_word >> 10) as u8);
    let refinement_template = if reader.is_pdfium() {
        RefinementTemplate::from_byte(u8::from((flags_word >> 12) & 3 != 0))
    } else {
        RefinementTemplate::from_byte((flags_word >> 12) as u8)
    };

    let flags = SymbolDictionaryFlags {
        use_huffman,
        use_refagg,
        delta_height_table,
        delta_width_table,
        collective_bitmap_size_table,
        aggregate_instance_table,
        bitmap_context_used,
        bitmap_context_retained,
        template,
        refinement_template,
    };

    let at_pixels = if !use_huffman {
        parse_adaptive_template_pixels(reader, template, false)?
    } else {
        // Unused.
        [AdaptiveTemplatePixel::default(); 4]
    };

    let refinement_at_pixels = if use_refagg && refinement_template == RefinementTemplate::Template0
    {
        parse_refinement_at_pixels(reader)?
    } else {
        Vec::new()
    };
    let num_exported_symbols = reader.read_u32().ok_or(ParseError::UnexpectedEof)?;
    let num_new_symbols = reader.read_u32().ok_or(ParseError::UnexpectedEof)?;

    let excessive = if reader.is_pdfium() {
        num_exported_symbols > (1 << 20) || num_new_symbols > (1 << 20)
    } else {
        num_exported_symbols.saturating_add(num_new_symbols) > MAX_SYMBOLS
    };
    if excessive {
        bail!(OverflowError::SymbolCount)
    }

    let data = reader.tail().ok_or(ParseError::UnexpectedEof)?;

    Ok(SymbolDictionaryHeader {
        pdfium: reader.is_pdfium(),
        flags,
        adaptive_template_pixels: at_pixels,
        refinement_at_pixels,
        num_exported_symbols,
        num_new_symbols,
        data,
    })
}
